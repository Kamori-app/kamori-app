package main

import (
	"encoding/json"
	"fmt"
	"strconv"
	"strings"

	"github.com/pulumi/pulumi-hcloud/sdk/go/hcloud"
	"github.com/pulumi/pulumi-terraform-provider/sdks/go/minio/v3/minio"
	"github.com/pulumi/pulumi/sdk/v3/go/pulumi"
	"github.com/pulumi/pulumi/sdk/v3/go/pulumi/config"
)

const (
	sshPort                   = "2022"
	defaultAppServerType      = "cx23"
	defaultOpsServerType      = "cx23"
	defaultDatabaseServerType = "cx33"
	hostProvisioningRetire    = "retire"
	hostProvisioningReplace   = "replace"
	hostProvisioningProtect   = "protect"

	// The generated Go SDK documents camelCase values, but the bridged hcloud
	// provider validates the Terraform wire value.
	loadBalancerAlgorithm = "least_connections"
)

type nodeSpec struct {
	name       string
	role       string
	serverType string
	location   string
	privateIP  string
}

func validateHostProvisioningPhase(value string) error {
	switch value {
	case hostProvisioningRetire, hostProvisioningReplace, hostProvisioningProtect:
		return nil
	default:
		return fmt.Errorf("hostProvisioningPhase must be %q, %q, or %q", hostProvisioningRetire, hostProvisioningReplace, hostProvisioningProtect)
	}
}

type cloudEnvSecrets struct {
	databasePassword     string
	valkeyPassword       string
	jwtSecret            string
	adminTotpKek         string
	authTotpKek          string
	objectStoreKeyID     string
	objectStoreSecretKey string
	metricsBearerToken   string
}

type appHostSecrets struct {
	opaqueServerSetup         string
	refreshRotationKey        string
	runtimeEnv                string
	postgresCACertificate     string
	postgresClientCertificate string
	postgresClientKey         string
}

func envLine(name, value string) string {
	return name + "=" + strconv.Quote(value) + "\n"
}

func renderCloudEnv(secrets cloudEnvSecrets, endpoint, region, bucket string) string {
	return "KAMORI_BIND_ADDR=0.0.0.0:8080\n" +
		envLine("KAMORI_DATABASE_URL", databaseConnectionURL(secrets.databasePassword)) +
		"KAMORI_DATABASE_MAX_CONNECTIONS=20\n" +
		envLine("KAMORI_VALKEY_URL", valkeyConnectionURL(secrets.valkeyPassword)) +
		"KAMORI_VALKEY_KEY_PREFIX=kamori:production:\n" +
		envLine("KAMORI_JWT_SECRET", secrets.jwtSecret) +
		"KAMORI_JWT_ISSUER=https://api.kamori.app\n" +
		"KAMORI_JWT_AUDIENCE=kamori-clients\n" +
		"KAMORI_ACCESS_TOKEN_TTL_SECONDS=300\n" +
		"KAMORI_REFRESH_TOKEN_TTL_SECONDS=2592000\n" +
		"KAMORI_JWT_PREAUTH_TTL_SECONDS=300\n" +
		"KAMORI_JWT_ACCOUNT_RECOVERY_TTL_SECONDS=600\n" +
		"KAMORI_OPAQUE_SERVER_SETUP_FILE=/run/secrets/opaque-server-setup\n" +
		"KAMORI_ALLOW_EPHEMERAL_OPAQUE_SETUP=false\n" +
		"KAMORI_REFRESH_ROTATION_KEY_FILE=/run/secrets/refresh-rotation-key\n" +
		"KAMORI_WEBAUTHN_RP_ID=kamori.app\n" +
		"KAMORI_WEBAUTHN_RP_ORIGIN=https://app.kamori.app\n" +
		"KAMORI_WEBAUTHN_RP_NAME=Kamori\n" +
		"KAMORI_ADMIN_WEBAUTHN_RP_ORIGIN=https://admin.kamori.app\n" +
		"KAMORI_ADMIN_WEBAUTHN_RP_NAME=Kamori Admin\n" +
		envLine("KAMORI_ADMIN_TOTP_KEK", secrets.adminTotpKek) +
		envLine("KAMORI_AUTH_TOTP_KEK", secrets.authTotpKek) +
		"KAMORI_ENABLE_TOTP=true\n" +
		"KAMORI_CORS_ALLOW_ORIGINS=https://app.kamori.app,https://admin.kamori.app,tauri://localhost\n" +
		"KAMORI_CORS_ALLOW_METHODS=GET,POST,DELETE,OPTIONS\n" +
		"KAMORI_CORS_ALLOW_HEADERS=authorization,content-type,accept,x-kamori-refresh-transport,x-kamori-csrf-token\n" +
		"KAMORI_CORS_ALLOW_CREDENTIALS=true\n" +
		"KAMORI_WEB_REFRESH_COOKIE_NAME=__Host-kamori_rt\n" +
		"KAMORI_WEB_CSRF_COOKIE_NAME=__Host-kamori_csrf\n" +
		"KAMORI_WEB_REFRESH_COOKIE_PATH=/\n" +
		"KAMORI_WEB_REFRESH_COOKIE_SECURE=true\n" +
		"KAMORI_WEB_REFRESH_COOKIE_SAMESITE=lax\n" +
		"KAMORI_REGISTRATION_ENABLED=false\n" +
		"KAMORI_BETA_ACCOUNT_LIMIT=1000\n" +
		"KAMORI_MAX_BLOB_BYTES=26214400\n" +
		"KAMORI_ACCOUNT_STORAGE_BYTES=5000000000\n" +
		"KAMORI_OWNER_MONTHLY_EGRESS_BYTES=10000000000\n" +
		"KAMORI_OWNER_ROLLING_24H_EGRESS_BYTES=2000000000\n" +
		"KAMORI_OWNER_CONCURRENT_BLOB_DOWNLOADS=2\n" +
		"KAMORI_BLOB_DOWNLOAD_BYTES_PER_SECOND=1250000\n" +
		"KAMORI_GLOBAL_NONESSENTIAL_EGRESS_STOP_BYTES=16000000000000\n" +
		"KAMORI_GLOBAL_EMERGENCY_EGRESS_BREAKER_BYTES=19000000000000\n" +
		envLine("KAMORI_OBJECT_STORE_ENDPOINT", "https://"+strings.TrimPrefix(endpoint, "https://")) +
		envLine("KAMORI_OBJECT_STORE_REGION", region) +
		envLine("KAMORI_OBJECT_STORE_BUCKET", bucket) +
		envLine("KAMORI_OBJECT_STORE_ACCESS_KEY_ID", secrets.objectStoreKeyID) +
		envLine("KAMORI_OBJECT_STORE_SECRET_ACCESS_KEY", secrets.objectStoreSecretKey) +
		"KAMORI_OBJECT_STORE_ALLOW_HTTP=false\n" +
		"KAMORI_OBJECT_STORE_VIRTUAL_HOSTED_STYLE=false\n" +
		envLine("KAMORI_METRICS_BEARER_TOKEN", secrets.metricsBearerToken) +
		"KAMORI_AUTH_RATE_LIMIT_PER_MINUTE=30\n" +
		"KAMORI_API_RATE_LIMIT_PER_MINUTE=1200\n" +
		"RUST_LOG=cloud_server=info,tower_http=warn\n"
}

func indentCloudConfigBlock(value string) string {
	return "      " + strings.ReplaceAll(strings.TrimSuffix(value, "\n"), "\n", "\n      ")
}

func main() {
	pulumi.Run(func(ctx *pulumi.Context) error {
		cfg := config.New(ctx, "kamori")
		hostProvisioningPhase := cfg.Get("hostProvisioningPhase")
		if hostProvisioningPhase == "" {
			hostProvisioningPhase = hostProvisioningRetire
		}
		if err := validateHostProvisioningPhase(hostProvisioningPhase); err != nil {
			return err
		}
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
		drBucketName, err := hetznerDRBucketName(ctx.Stack())
		if err != nil {
			return err
		}
		sshKeys := splitRequiredCSV(cfg.Require("sshKeys"), "sshKeys")

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

		appPlacement, err := hcloud.NewPlacementGroup(ctx, "app-placement", &hcloud.PlacementGroupArgs{
			Name: pulumi.String("kamori-beta-app"), Type: pulumi.String("spread"), Labels: commonLabels("app"),
		}, opts)
		if err != nil {
			return err
		}

		hosts, err := provisionHosts(ctx, cfg, provider, network, subnet, appPlacement, sshKeys, drBucketName, hostProvisioningPhase)
		if err != nil {
			return err
		}
		servers := hosts.servers

		loadBalancer, err := hcloud.NewLoadBalancer(ctx, "public-load-balancer", &hcloud.LoadBalancerArgs{
			Name: pulumi.String("kamori-beta-public"), LoadBalancerType: pulumi.String("lb11"), Location: pulumi.String("nbg1"), DeleteProtection: pulumi.Bool(true), Labels: commonLabels("public-edge"), Algorithm: &hcloud.LoadBalancerAlgorithmArgs{Type: pulumi.String(loadBalancerAlgorithm)},
		}, opts)
		if err != nil {
			return err
		}
		publicEdge, err := provisionPublicDNSAndTLS(ctx, cfg, provider, loadBalancer)
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
			Http:        &hcloud.LoadBalancerServiceHttpArgs{Certificates: pulumi.IntArray{idToInt(publicEdge.certificate.ID())}, RedirectHttp: pulumi.Bool(true), StickySessions: pulumi.Bool(false), TimeoutIdle: pulumi.Int(60)},
			HealthCheck: &hcloud.LoadBalancerServiceHealthCheckArgs{Protocol: pulumi.String("http"), Port: pulumi.Int(8080), Interval: pulumi.Int(15), Timeout: pulumi.Int(5), Retries: pulumi.Int(3), Http: &hcloud.LoadBalancerServiceHealthCheckHttpArgs{Path: pulumi.String("/health/ready"), StatusCodes: pulumi.StringArray{pulumi.String("200")}}},
		}, opts, pulumi.DependsOn([]pulumi.Resource{lbNetwork, publicEdge.certificate}))
		if err != nil {
			return err
		}

		drProvider, err := minio.NewProvider(ctx, "hetzner-object-storage", &minio.ProviderArgs{
			MinioServer:           pulumi.String(hetznerObjectEndpoint),
			MinioRegion:           pulumi.String(hetznerObjectLocation),
			MinioUser:             cfg.RequireSecret("hetznerObjectAccessKey").ToStringPtrOutput(),
			MinioPassword:         cfg.RequireSecret("hetznerObjectSecretKey").ToStringPtrOutput(),
			MinioSsl:              pulumi.Bool(true),
			S3CompatMode:          pulumi.Bool(hetznerObjectS3CompatMode),
			SkipBucketTagging:     pulumi.Bool(true),
			MaxRetries:            pulumi.Float64(6),
			RequestTimeoutSeconds: pulumi.Float64(30),
		})
		if err != nil {
			return err
		}
		drBucket, err := minio.NewS3Bucket(ctx, "dr-blobs", &minio.S3BucketArgs{
			Bucket: pulumi.String(drBucketName), Acl: pulumi.String("private"), ForceDestroy: pulumi.Bool(false),
		},
			pulumi.Provider(drProvider),
			pulumi.Protect(true),
		)
		if err != nil {
			return err
		}

		ctx.Export("loadBalancerIPv4", loadBalancer.Ipv4)
		ctx.Export("loadBalancerIPv6", loadBalancer.Ipv6)
		ctx.Export("publicDNSNameservers", publicEdge.certificateZone.AuthoritativeNameservers.Assigneds())
		ctx.Export("tlsCertificateID", publicEdge.certificate.ID())
		ctx.Export("tlsCertificateNotValidAfter", publicEdge.certificate.NotValidAfter)
		ctx.Export("primaryBlobBucket", pulumi.String(backblazePrimaryBucket))
		ctx.Export("postgresBackupBucket", pulumi.String(backblazePostgresBackupBucket))
		ctx.Export("drBlobBucket", drBucket.Bucket)
		ctx.Export("drBlobLocation", pulumi.String(hetznerObjectLocation))
		ctx.Export("drBlobEndpoint", pulumi.String(hetznerObjectEndpoint))
		ctx.Export("budgetGuardrails", pulumi.String(encodedGuardrails))
		ctx.Export("databasePrimaryPrivateIP", pulumi.String(databasePrimaryPrivateIP))
		ctx.Export("appOnePrivateIP", pulumi.String("10.42.0.11"))
		ctx.Export("appTwoPrivateIP", pulumi.String("10.42.0.12"))
		ctx.Export("opsPrivateIP", pulumi.String(valkeyPrivateIP))
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

func stringsToInputs(values []string) pulumi.StringArray {
	result := make(pulumi.StringArray, 0, len(values))
	for _, value := range values {
		result = append(result, pulumi.String(value))
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

func baseCloudInit(role string, secrets appHostSecrets) string {
	extraPackages := ""
	extraCommands := ""
	opaqueSetupFile := ""
	switch role {
	case "app":
		extraPackages = "  - docker.io\n"
		extraCommands = "  - [systemctl, enable, --now, docker]\n  - [chown, '10001:10001', /etc/kamori/cloud.env]\n  - [chown, '10001:10001', /etc/kamori/secrets/opaque-server-setup]\n  - [chown, '10001:10001', /etc/kamori/secrets/refresh-rotation-key]\n  - [chown, '10001:10001', /etc/kamori/postgres-ca.crt]\n  - [chown, '10001:10001', /etc/kamori/postgres-client.crt]\n  - [chown, '10001:10001', /etc/kamori/postgres-client.key]\n"
		opaqueSetupFile = fmt.Sprintf(`  - path: /etc/kamori/secrets/opaque-server-setup
    owner: root:root
    permissions: '0400'
    content: |
      %s
  - path: /etc/kamori/secrets/refresh-rotation-key
    owner: root:root
    permissions: '0400'
    content: |
      %s
  - path: /etc/kamori/cloud.env
    owner: root:root
    permissions: '0400'
    content: |
%s
  - path: /etc/kamori/postgres-ca.crt
    owner: root:root
    permissions: '0444'
    content: |
%s
  - path: /etc/kamori/postgres-client.crt
    owner: root:root
    permissions: '0444'
    content: |
%s
  - path: /etc/kamori/postgres-client.key
    owner: root:root
    permissions: '0400'
    content: |
%s
`, secrets.opaqueServerSetup, secrets.refreshRotationKey, indentCloudConfigBlock(secrets.runtimeEnv), indentCloudConfigBlock(secrets.postgresCACertificate), indentCloudConfigBlock(secrets.postgresClientCertificate), indentCloudConfigBlock(secrets.postgresClientKey))
	case "ops":
		extraPackages = "  - docker.io\n  - docker-compose-v2\n"
		extraCommands = "  - [systemctl, enable, --now, docker]\n"
	case "db-primary":
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
%s  - path: /etc/sysctl.d/60-kamori-hardening.conf
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
      Port %s
      PasswordAuthentication no
      KbdInteractiveAuthentication no
      PermitRootLogin prohibit-password
      X11Forwarding no
  - path: /etc/fail2ban/jail.d/kamori-sshd.local
    owner: root:root
    permissions: '0644'
    content: |
      [sshd]
      enabled = true
      port = %s
      backend = systemd
runcmd:
  - [systemctl, enable, --now, unattended-upgrades]
  - [systemctl, enable, --now, fail2ban]
  - [systemctl, enable, --now, chrony]
  - [systemctl, enable, --now, prometheus-node-exporter]
  - [sshd, -t]
  - [systemctl, reload, ssh]
%s  - [sysctl, --system]
`, extraPackages, role, opaqueSetupFile, sshPort, sshPort, extraCommands)
}
