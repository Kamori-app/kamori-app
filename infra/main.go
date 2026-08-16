package main

import (
	"encoding/json"
	"fmt"
	"strconv"
	"strings"

	"github.com/pulumi/pulumi-hcloud/sdk/go/hcloud"
	"github.com/pulumi/pulumi-minio/sdk/go/minio"
	"github.com/pulumi/pulumi/sdk/v3/go/pulumi"
	"github.com/pulumi/pulumi/sdk/v3/go/pulumi/config"
)

type nodeSpec struct {
	name       string
	role       string
	serverType string
	location   string
	privateIP  string
}

func main() {
	pulumi.Run(func(ctx *pulumi.Context) error {
		cfg := config.New(ctx, "kamori")
		provider, err := hcloud.NewProvider(ctx, "hetzner", &hcloud.ProviderArgs{
			Token: cfg.RequireSecret("hcloudToken").ToStringPtrOutput(),
		})
		if err != nil {
			return err
		}
		opts := pulumi.Provider(provider)

		guardrails := defaultBudgetGuardrails()
		if err := guardrails.validate(); err != nil {
			return err
		}
		encodedGuardrails, err := json.Marshal(guardrails)
		if err != nil {
			return err
		}

		sshKeys := splitRequiredCSV(cfg.Require("sshKeys"), "sshKeys")
		adminCIDRs := splitRequiredCSV(cfg.Require("adminCidrs"), "adminCidrs")
		certificateIDs, err := parseRequiredInts(cfg.Require("tlsCertificateIds"))
		if err != nil {
			return fmt.Errorf("tlsCertificateIds: %w", err)
		}

		network, err := hcloud.NewNetwork(ctx, "private-network", &hcloud.NetworkArgs{
			Name:             pulumi.String("kamori-beta"),
			IpRange:          pulumi.String("10.42.0.0/16"),
			DeleteProtection: pulumi.Bool(true),
			Labels:           commonLabels("network"),
		}, opts)
		if err != nil {
			return err
		}
		subnet, err := hcloud.NewNetworkSubnet(ctx, "private-subnet", &hcloud.NetworkSubnetArgs{
			NetworkId:   idToInt(network.ID()),
			Type:        pulumi.String("cloud"),
			NetworkZone: pulumi.String("eu-central"),
			IpRange:     pulumi.String("10.42.0.0/24"),
		}, opts, pulumi.DependsOn([]pulumi.Resource{network}))
		if err != nil {
			return err
		}

		firewall, err := hcloud.NewFirewall(ctx, "host-firewall", &hcloud.FirewallArgs{
			Name:   pulumi.String("kamori-beta-hosts"),
			Labels: commonLabels("firewall"),
			Rules: hcloud.FirewallRuleArray{
				&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("22"), SourceIps: stringsToInputs(adminCIDRs), Description: pulumi.String("operator SSH")},
				&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("8080"), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.0/16")}, Description: pulumi.String("load balancer to app")},
				&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("5432"), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.0/16")}, Description: pulumi.String("private PostgreSQL")},
				&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("6379"), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.0/16")}, Description: pulumi.String("private Valkey")},
				&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("9100"), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.0/16")}, Description: pulumi.String("private metrics")},
				&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("9090"), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.0/16")}, Description: pulumi.String("private Prometheus")},
			},
		}, opts)
		if err != nil {
			return err
		}

		appPlacement, err := hcloud.NewPlacementGroup(ctx, "app-placement", &hcloud.PlacementGroupArgs{
			Name: pulumi.String("kamori-beta-app"), Type: pulumi.String("spread"), Labels: commonLabels("app"),
		}, opts)
		if err != nil {
			return err
		}

		nodes := []nodeSpec{
			{name: "app-1", role: "app", serverType: cfg.Get("appServerType"), location: "nbg1", privateIP: "10.42.0.11"},
			{name: "app-2", role: "app", serverType: cfg.Get("appServerType"), location: "fsn1", privateIP: "10.42.0.12"},
			{name: "db-primary", role: "db-primary", serverType: cfg.Get("dbServerType"), location: "nbg1", privateIP: "10.42.0.21"},
			{name: "db-standby", role: "db-standby", serverType: cfg.Get("dbServerType"), location: "fsn1", privateIP: "10.42.0.22"},
			{name: "ops", role: "ops", serverType: cfg.Get("opsServerType"), location: "hel1", privateIP: "10.42.0.31"},
		}
		servers := make(map[string]*hcloud.Server, len(nodes))
		for _, spec := range nodes {
			if spec.serverType == "" {
				switch spec.role {
				case "app":
					spec.serverType = "cax11"
				case "ops":
					spec.serverType = "cax11"
				default:
					spec.serverType = "cax21"
				}
			}
			args := &hcloud.ServerArgs{
				Name:              pulumi.String("kamori-beta-" + spec.name),
				Image:             pulumi.String("ubuntu-24.04"),
				ServerType:        pulumi.String(spec.serverType),
				Location:          pulumi.String(spec.location),
				SshKeys:           stringsToInputs(sshKeys),
				FirewallIds:       pulumi.IntArray{idToInt(firewall.ID())},
				Backups:           pulumi.Bool(true),
				UserData:          pulumi.String(baseCloudInit(spec.role)),
				Labels:            commonLabels(spec.role),
				PublicNets:        hcloud.ServerPublicNetArray{&hcloud.ServerPublicNetArgs{Ipv4Enabled: pulumi.Bool(true), Ipv6Enabled: pulumi.Bool(true)}},
				Networks:          hcloud.ServerNetworkTypeArray{&hcloud.ServerNetworkTypeArgs{SubnetId: subnet.ID(), Ip: pulumi.String(spec.privateIP), AliasIps: pulumi.StringArray{}}},
				DeleteProtection:  pulumi.Bool(true),
				RebuildProtection: pulumi.Bool(true),
			}
			if spec.role == "app" {
				args.PlacementGroupId = idToInt(appPlacement.ID()).ToIntPtrOutput()
			}
			server, err := hcloud.NewServer(ctx, spec.name, args, opts, pulumi.DependsOn([]pulumi.Resource{subnet, firewall}))
			if err != nil {
				return err
			}
			servers[spec.name] = server
		}

		for _, name := range []string{"db-primary", "db-standby"} {
			_, err = hcloud.NewVolume(ctx, name+"-data", &hcloud.VolumeArgs{
				Name: pulumi.String("kamori-beta-" + name + "-data"), Size: pulumi.Int(80), ServerId: idToInt(servers[name].ID()).ToIntPtrOutput(), Format: pulumi.String("ext4"), Automount: pulumi.Bool(true), DeleteProtection: pulumi.Bool(true), Labels: commonLabels("postgres-data"),
			}, opts, pulumi.Protect(true))
			if err != nil {
				return err
			}
		}

		loadBalancer, err := hcloud.NewLoadBalancer(ctx, "public-load-balancer", &hcloud.LoadBalancerArgs{
			Name: pulumi.String("kamori-beta-public"), LoadBalancerType: pulumi.String("lb11"), Location: pulumi.String("nbg1"), DeleteProtection: pulumi.Bool(true), Labels: commonLabels("public-edge"), Algorithm: &hcloud.LoadBalancerAlgorithmArgs{Type: pulumi.String("leastConnections")},
		}, opts)
		if err != nil {
			return err
		}
		lbNetwork, err := hcloud.NewLoadBalancerNetwork(ctx, "load-balancer-network", &hcloud.LoadBalancerNetworkArgs{
			LoadBalancerId: idToInt(loadBalancer.ID()), NetworkId: idToInt(network.ID()).ToIntPtrOutput(), EnablePublicInterface: pulumi.Bool(true), Ip: pulumi.String("10.42.0.5"),
		}, opts, pulumi.DependsOn([]pulumi.Resource{subnet}))
		if err != nil {
			return err
		}
		for _, name := range []string{"app-1", "app-2"} {
			_, err = hcloud.NewLoadBalancerTarget(ctx, "target-"+name, &hcloud.LoadBalancerTargetArgs{
				LoadBalancerId: idToInt(loadBalancer.ID()), Type: pulumi.String("server"), ServerId: idToInt(servers[name].ID()).ToIntPtrOutput(), UsePrivateIp: pulumi.Bool(true),
			}, opts, pulumi.DependsOn([]pulumi.Resource{lbNetwork, servers[name]}))
			if err != nil {
				return err
			}
		}
		_, err = hcloud.NewLoadBalancerService(ctx, "https-service", &hcloud.LoadBalancerServiceArgs{
			LoadBalancerId: loadBalancer.ID(), Protocol: pulumi.String("https"), ListenPort: pulumi.Int(443), DestinationPort: pulumi.Int(8080),
			Http:        &hcloud.LoadBalancerServiceHttpArgs{Certificates: intsToInputs(certificateIDs), RedirectHttp: pulumi.Bool(true), StickySessions: pulumi.Bool(false), TimeoutIdle: pulumi.Int(60)},
			HealthCheck: &hcloud.LoadBalancerServiceHealthCheckArgs{Protocol: pulumi.String("http"), Port: pulumi.Int(8080), Interval: pulumi.Int(15), Timeout: pulumi.Int(5), Retries: pulumi.Int(3), Http: &hcloud.LoadBalancerServiceHealthCheckHttpArgs{Path: pulumi.String("/health/ready"), StatusCodes: pulumi.StringArray{pulumi.String("200")}}},
		}, opts, pulumi.DependsOn([]pulumi.Resource{lbNetwork}))
		if err != nil {
			return err
		}

		b2Provider, err := minio.NewProvider(ctx, "b2", &minio.ProviderArgs{
			MinioServer: pulumi.String(cfg.Require("b2Endpoint")), MinioRegion: pulumi.String(cfg.Require("b2Region")), MinioUser: cfg.RequireSecret("b2InfraKeyId").ToStringPtrOutput(), MinioPassword: cfg.RequireSecret("b2InfraApplicationKey").ToStringPtrOutput(), MinioSsl: pulumi.Bool(true),
		})
		if err != nil {
			return err
		}
		postgresBackupBucket, err := minio.NewS3Bucket(ctx, "postgres-backups", &minio.S3BucketArgs{
			Bucket: pulumi.String(cfg.Require("b2PostgresBackupBucket")), Acl: pulumi.String("private"), ForceDestroy: pulumi.Bool(false),
		}, pulumi.Provider(b2Provider), pulumi.Protect(true))
		if err != nil {
			return err
		}
		b2Bucket, err := minio.NewS3Bucket(ctx, "primary-blobs", &minio.S3BucketArgs{
			Bucket: pulumi.String(cfg.Require("b2Bucket")), Acl: pulumi.String("private"), ForceDestroy: pulumi.Bool(false),
		}, pulumi.Provider(b2Provider), pulumi.Protect(true))
		if err != nil {
			return err
		}

		drProvider, err := minio.NewProvider(ctx, "hetzner-object-storage", &minio.ProviderArgs{
			MinioServer: pulumi.String(cfg.Require("hetznerObjectEndpoint")), MinioRegion: pulumi.String(cfg.Require("hetznerObjectRegion")), MinioUser: cfg.RequireSecret("hetznerObjectAccessKey").ToStringPtrOutput(), MinioPassword: cfg.RequireSecret("hetznerObjectSecretKey").ToStringPtrOutput(), MinioSsl: pulumi.Bool(true),
		})
		if err != nil {
			return err
		}
		drBucket, err := minio.NewS3Bucket(ctx, "dr-blobs", &minio.S3BucketArgs{
			Bucket: pulumi.String(cfg.Require("hetznerObjectBucket")), Acl: pulumi.String("private"), ForceDestroy: pulumi.Bool(false),
		}, pulumi.Provider(drProvider), pulumi.Protect(true))
		if err != nil {
			return err
		}

		ctx.Export("loadBalancerIPv4", loadBalancer.Ipv4)
		ctx.Export("loadBalancerIPv6", loadBalancer.Ipv6)
		ctx.Export("primaryBlobBucket", b2Bucket.Bucket)
		ctx.Export("postgresBackupBucket", postgresBackupBucket.Bucket)
		ctx.Export("drBlobBucket", drBucket.Bucket)
		ctx.Export("budgetGuardrails", pulumi.String(encodedGuardrails))
		ctx.Export("databasePrimaryPrivateIP", pulumi.String("10.42.0.21"))
		ctx.Export("databaseStandbyPrivateIP", pulumi.String("10.42.0.22"))
		ctx.Export("appOnePrivateIP", pulumi.String("10.42.0.11"))
		ctx.Export("appTwoPrivateIP", pulumi.String("10.42.0.12"))
		ctx.Export("opsPrivateIP", pulumi.String("10.42.0.31"))
		ctx.Export("opsPublicIPv4", servers["ops"].Ipv4Address)
		return nil
	})
}

func commonLabels(role string) pulumi.StringMap {
	return pulumi.StringMap{"service": pulumi.String("kamori"), "environment": pulumi.String("beta"), "role": pulumi.String(role), "managed-by": pulumi.String("pulumi")}
}

func splitRequiredCSV(value, name string) []string {
	items := strings.Split(value, ",")
	result := make([]string, 0, len(items))
	for _, item := range items {
		if trimmed := strings.TrimSpace(item); trimmed != "" {
			result = append(result, trimmed)
		}
	}
	if len(result) == 0 {
		panic(name + " must contain at least one value")
	}
	return result
}

func parseRequiredInts(value string) ([]int, error) {
	parts := splitRequiredCSV(value, "tlsCertificateIds")
	result := make([]int, 0, len(parts))
	for _, part := range parts {
		value, err := strconv.Atoi(part)
		if err != nil || value <= 0 {
			return nil, fmt.Errorf("%q is not a positive certificate id", part)
		}
		result = append(result, value)
	}
	return result, nil
}

func stringsToInputs(values []string) pulumi.StringArray {
	result := make(pulumi.StringArray, 0, len(values))
	for _, value := range values {
		result = append(result, pulumi.String(value))
	}
	return result
}

func intsToInputs(values []int) pulumi.IntArray {
	result := make(pulumi.IntArray, 0, len(values))
	for _, value := range values {
		result = append(result, pulumi.Int(value))
	}
	return result
}

func idToInt(id pulumi.IDOutput) pulumi.IntOutput {
	return id.ApplyT(func(value pulumi.ID) (int, error) {
		parsed, err := strconv.Atoi(string(value))
		if err != nil {
			return 0, fmt.Errorf("provider returned non-numeric id %q: %w", value, err)
		}
		return parsed, nil
	}).(pulumi.IntOutput)
}

func baseCloudInit(role string) string {
	extraPackages := ""
	extraCommands := ""
	switch role {
	case "app":
		extraPackages = "  - docker.io\n"
		extraCommands = "  - [systemctl, enable, --now, docker]\n"
	case "ops":
		extraPackages = "  - docker.io\n  - docker-compose-v2\n"
		extraCommands = "  - [systemctl, enable, --now, docker]\n"
	case "db-primary", "db-standby":
		extraPackages = "  - postgresql\n  - postgresql-client\n  - pgbackrest\n"
	}
	return fmt.Sprintf(`#cloud-config
package_update: true
package_upgrade: true
packages:
  - unattended-upgrades
  - fail2ban
  - ca-certificates
  - curl
  - jq
  - chrony
  - prometheus-node-exporter
%swrite_files:
  - path: /etc/kamori/node-role
    owner: root:root
    permissions: '0644'
    content: %s
  - path: /etc/sysctl.d/60-kamori-hardening.conf
    owner: root:root
    permissions: '0644'
    content: |
      kernel.kptr_restrict=2
      kernel.dmesg_restrict=1
      fs.protected_hardlinks=1
      fs.protected_symlinks=1
  - path: /etc/ssh/sshd_config.d/60-kamori-hardening.conf
    owner: root:root
    permissions: '0644'
    content: |
      PasswordAuthentication no
      KbdInteractiveAuthentication no
      PermitRootLogin prohibit-password
      X11Forwarding no
runcmd:
  - [systemctl, enable, --now, unattended-upgrades]
  - [systemctl, enable, --now, fail2ban]
  - [systemctl, enable, --now, chrony]
  - [systemctl, enable, --now, prometheus-node-exporter]
  - [systemctl, reload, ssh]
%s  - [sysctl, --system]
`, extraPackages, role, extraCommands)
}
