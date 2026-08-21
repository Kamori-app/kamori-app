package main

import (
	"github.com/pulumi/pulumi-hcloud/sdk/go/hcloud"
	"github.com/pulumi/pulumi/sdk/v3/go/pulumi"
	"github.com/pulumi/pulumi/sdk/v3/go/pulumi/config"
)

type hostResources struct {
	servers map[string]*hcloud.Server
	sshPKI  *sshPKI
}

func provisionHosts(
	ctx *pulumi.Context,
	cfg *config.Config,
	provider *hcloud.Provider,
	network *hcloud.Network,
	subnet *hcloud.NetworkSubnet,
	appPlacement *hcloud.PlacementGroup,
	sshKeys []string,
	drBucketName string,
	phase string,
) (*hostResources, error) {
	if phase == hostProvisioningRetire {
		return provisionRetiringHosts(ctx, cfg, provider, subnet, appPlacement, sshKeys)
	}
	return provisionAutomatedHosts(ctx, cfg, provider, network, subnet, appPlacement, sshKeys, drBucketName, phase == hostProvisioningProtect)
}

func provisionRetiringHosts(
	ctx *pulumi.Context,
	cfg *config.Config,
	provider *hcloud.Provider,
	subnet *hcloud.NetworkSubnet,
	appPlacement *hcloud.PlacementGroup,
	sshKeys []string,
) (*hostResources, error) {
	opts := pulumi.Provider(provider)
	firewall, err := hcloud.NewFirewall(ctx, "host-firewall", &hcloud.FirewallArgs{
		Name:   pulumi.String("kamori-beta-hosts"),
		Labels: commonLabels("firewall"),
		Rules: hcloud.FirewallRuleArray{
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String(sshPort), SourceIps: pulumi.StringArray{pulumi.String(valkeyPrivateIP + "/32")}, Description: pulumi.String("ops runner SSH")},
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("8080"), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.0/16")}, Description: pulumi.String("load balancer to app")},
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("5432"), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.0/16")}, Description: pulumi.String("private PostgreSQL")},
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("6379"), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.0/16")}, Description: pulumi.String("private Valkey")},
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("9100"), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.0/16")}, Description: pulumi.String("private metrics")},
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("9090"), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.0/16")}, Description: pulumi.String("private Prometheus")},
		},
	}, opts)
	if err != nil {
		return nil, err
	}
	operatorFirewall, err := hcloud.NewFirewall(ctx, "operator-firewall", &hcloud.FirewallArgs{
		Name:   pulumi.String("kamori-beta-operator-access"),
		Labels: commonLabels("operator-access"),
		Rules: hcloud.FirewallRuleArray{
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String(sshPort), SourceIps: pulumi.StringArray{pulumi.String("0.0.0.0/0"), pulumi.String("::/0")}, Description: pulumi.String("operator SSH to ops bastion")},
		},
	}, opts)
	if err != nil {
		return nil, err
	}

	opaqueServerSetup := cfg.RequireSecret("opaqueServerSetup")
	refreshRotationKey := cfg.RequireSecret("refreshRotationKey")
	postgresCACertificate := pulumi.String(cfg.Require("postgresCaCertificate"))
	postgresClientCertificate := pulumi.String(cfg.Require("postgresClientCertificate"))
	postgresClientKey := cfg.RequireSecret("postgresClientKey")
	appRuntimeEnv := renderApplicationEnvironment(cfg)

	nodes := hostNodeSpecs(cfg)
	servers := make(map[string]*hcloud.Server, len(nodes))
	for _, spec := range nodes {
		firewallIDs := pulumi.IntArray{idToInt(firewall.ID())}
		if spec.role == "ops" {
			firewallIDs = append(firewallIDs, idToInt(operatorFirewall.ID()))
		}
		args := standardServerArgs(spec, subnet, sshKeys, firewallIDs)
		args.PublicNets = hcloud.ServerPublicNetArray{&hcloud.ServerPublicNetArgs{Ipv4Enabled: pulumi.Bool(true), Ipv6Enabled: pulumi.Bool(true)}}
		args.DeleteProtection = pulumi.Bool(false)
		args.RebuildProtection = pulumi.Bool(false)
		args.UserData = pulumi.String(baseCloudInit(spec.role, appHostSecrets{}))
		if spec.role == "app" {
			args.PlacementGroupId = idToInt(appPlacement.ID()).ToIntPtrOutput()
			role := spec.role
			args.UserData = pulumi.All(
				opaqueServerSetup,
				refreshRotationKey,
				appRuntimeEnv,
				postgresCACertificate,
				postgresClientCertificate,
				postgresClientKey,
			).ApplyT(func(values []interface{}) string {
				return baseCloudInit(role, appHostSecrets{
					opaqueServerSetup:         values[0].(string),
					refreshRotationKey:        values[1].(string),
					runtimeEnv:                values[2].(string),
					postgresCACertificate:     values[3].(string),
					postgresClientCertificate: values[4].(string),
					postgresClientKey:         values[5].(string),
				})
			}).(pulumi.StringOutput)
		}
		server, err := hcloud.NewServer(ctx, spec.name, args, opts, pulumi.DependsOn([]pulumi.Resource{subnet, firewall, operatorFirewall}))
		if err != nil {
			return nil, err
		}
		servers[spec.name] = server
	}

	_, err = hcloud.NewVolume(ctx, "db-primary-data", &hcloud.VolumeArgs{
		Name: pulumi.String("kamori-beta-db-primary-data"), Size: pulumi.Int(80), ServerId: idToInt(servers["db-primary"].ID()).ToIntPtrOutput(), Format: pulumi.String("ext4"), Automount: pulumi.Bool(true), DeleteProtection: pulumi.Bool(false), Labels: commonLabels("postgres-data"),
	}, opts)
	if err != nil {
		return nil, err
	}
	return &hostResources{servers: servers}, nil
}

func provisionAutomatedHosts(
	ctx *pulumi.Context,
	cfg *config.Config,
	provider *hcloud.Provider,
	network *hcloud.Network,
	subnet *hcloud.NetworkSubnet,
	appPlacement *hcloud.PlacementGroup,
	sshKeys []string,
	drBucketName string,
	protected bool,
) (*hostResources, error) {
	opts := pulumi.Provider(provider)
	passwords, err := provisionGeneratedPasswords(ctx)
	if err != nil {
		return nil, err
	}
	postgresIdentity, err := provisionPostgresPKI(ctx)
	if err != nil {
		return nil, err
	}
	nodes := hostNodeSpecs(cfg)
	hostNames := make([]string, 0, len(nodes))
	for _, spec := range nodes {
		hostNames = append(hostNames, "kamori-beta-"+spec.name)
	}
	sshIdentity, err := provisionSSHPKI(ctx, hostNames)
	if err != nil {
		return nil, err
	}

	appFirewall, err := hcloud.NewFirewall(ctx, "app-firewall", &hcloud.FirewallArgs{
		Name: pulumi.String("kamori-beta-app"), Labels: commonLabels("app-firewall"),
		Rules: hcloud.FirewallRuleArray{
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String(sshPort), SourceIps: pulumi.StringArray{pulumi.String(valkeyPrivateIP + "/32")}, Description: pulumi.String("SSH through ops bastion")},
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("8080"), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.5/32"), pulumi.String(valkeyPrivateIP + "/32")}, Description: pulumi.String("private load balancer and Prometheus")},
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("9100"), SourceIps: pulumi.StringArray{pulumi.String(valkeyPrivateIP + "/32")}, Description: pulumi.String("node metrics from ops")},
		},
	}, opts)
	if err != nil {
		return nil, err
	}
	databaseFirewall, err := hcloud.NewFirewall(ctx, "database-firewall", &hcloud.FirewallArgs{
		Name: pulumi.String("kamori-beta-database"), Labels: commonLabels("database-firewall"),
		Rules: hcloud.FirewallRuleArray{
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String(sshPort), SourceIps: pulumi.StringArray{pulumi.String(valkeyPrivateIP + "/32")}, Description: pulumi.String("operator SSH through ops")},
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String(databasePort), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.11/32"), pulumi.String("10.42.0.12/32"), pulumi.String(valkeyPrivateIP + "/32")}, Description: pulumi.String("PostgreSQL clients")},
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String("9100"), SourceIps: pulumi.StringArray{pulumi.String(valkeyPrivateIP + "/32")}, Description: pulumi.String("node metrics from ops")},
		},
	}, opts)
	if err != nil {
		return nil, err
	}
	opsFirewall, err := hcloud.NewFirewall(ctx, "ops-firewall", &hcloud.FirewallArgs{
		Name: pulumi.String("kamori-beta-ops"), Labels: commonLabels("ops-firewall"),
		Rules: hcloud.FirewallRuleArray{
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String(sshPort), SourceIps: pulumi.StringArray{pulumi.String("0.0.0.0/0")}, Description: pulumi.String("operator and protected deployment SSH")},
			&hcloud.FirewallRuleArgs{Direction: pulumi.String("in"), Protocol: pulumi.String("tcp"), Port: pulumi.String(valkeyPort), SourceIps: pulumi.StringArray{pulumi.String("10.42.0.11/32"), pulumi.String("10.42.0.12/32")}, Description: pulumi.String("Valkey from app nodes")},
		},
	}, opts)
	if err != nil {
		return nil, err
	}

	volumeOptions := []pulumi.ResourceOption{opts, pulumi.DeleteBeforeReplace(true)}
	if protected {
		volumeOptions = append(volumeOptions, pulumi.Protect(true))
	}
	dataVolume, err := hcloud.NewVolume(ctx, "db-primary-data", &hcloud.VolumeArgs{
		Name:             pulumi.String("kamori-beta-db-primary-data"),
		Size:             pulumi.Int(80),
		Location:         pulumi.String("nbg1"),
		Format:           pulumi.String("ext4"),
		DeleteProtection: pulumi.Bool(protected),
		Labels:           commonLabels("postgres-data"),
	}, volumeOptions...)
	if err != nil {
		return nil, err
	}

	opPrimaryIPOptions := []pulumi.ResourceOption{opts}
	if protected {
		opPrimaryIPOptions = append(opPrimaryIPOptions, pulumi.Protect(true))
	}
	opsPrimaryIP, err := hcloud.NewPrimaryIp(ctx, "ops-primary-ipv4", &hcloud.PrimaryIpArgs{
		Name:             pulumi.String("kamori-beta-ops"),
		Type:             pulumi.String("ipv4"),
		Location:         pulumi.String("hel1"),
		AutoDelete:       pulumi.Bool(false),
		DeleteProtection: pulumi.Bool(protected),
		Labels:           commonLabels("ops-public-ip"),
	}, opPrimaryIPOptions...)
	if err != nil {
		return nil, err
	}

	appRuntimeEnv := renderApplicationEnvironment(cfg)
	postgresEnvironment := pulumi.All(
		cfg.RequireSecret("databasePassword"),
		passwords.postgresJobs,
		cfg.RequireSecret("postgresBackupKeyId"),
		cfg.RequireSecret("postgresBackupApplicationKey"),
		passwords.pgBackRest,
	).ApplyT(func(values []interface{}) string {
		return renderPostgresEnvironment(values[0].(string), values[1].(string), values[2].(string), values[3].(string), values[4].(string))
	}).(pulumi.StringOutput)
	backupEnvironment := pulumi.All(
		cfg.RequireSecret("b2ReplicationKeyId"),
		cfg.RequireSecret("b2ReplicationApplicationKey"),
		cfg.RequireSecret("hetznerObjectAccessKey"),
		cfg.RequireSecret("hetznerObjectSecretKey"),
		passwords.postgresJobs,
	).ApplyT(func(values []interface{}) string {
		return renderBackupEnvironment(values[0].(string), values[1].(string), values[2].(string), values[3].(string), values[4].(string), drBucketName)
	}).(pulumi.StringOutput)

	userData := make(map[string]pulumi.StringOutput, len(nodes))
	for _, spec := range nodes {
		hostName := "kamori-beta-" + spec.name
		hostIdentity := sshIdentity.hosts[hostName]
		switch spec.role {
		case "app":
			userData[spec.name] = pulumi.All(
				hostIdentity.privateKey,
				hostIdentity.certificate,
				sshIdentity.deployPublicKey,
				appRuntimeEnv,
				cfg.RequireSecret("opaqueServerSetup"),
				cfg.RequireSecret("refreshRotationKey"),
				postgresIdentity.caCertificate,
				postgresIdentity.appClientCertificate,
				postgresIdentity.appClientPrivateKey,
			).ApplyT(func(values []interface{}) (string, error) {
				return renderAppCloudInit(appCloudInitMaterial{
					commonHostMaterial: commonHostMaterial{hostName: hostName, hostPrivateKey: values[0].(string), hostCertificate: values[1].(string)},
					deployPublicKey:    values[2].(string), cloudEnvironment: values[3].(string), opaqueServerSetup: values[4].(string), refreshRotationKey: values[5].(string),
					postgresCACertificate: values[6].(string), postgresClientCertificate: values[7].(string), postgresClientPrivateKey: values[8].(string),
				})
			}).(pulumi.StringOutput)
		case "ops":
			userData[spec.name] = pulumi.All(
				hostIdentity.privateKey,
				hostIdentity.certificate,
				sshIdentity.deployPublicKey,
				cfg.RequireSecret("valkeyPassword"),
				passwords.grafanaAdmin,
				cfg.RequireSecret("metricsBearerToken"),
				backupEnvironment,
				postgresIdentity.caCertificate,
				postgresIdentity.jobsClientCertificate,
				postgresIdentity.jobsClientPrivateKey,
			).ApplyT(func(values []interface{}) (string, error) {
				return renderOpsCloudInit(opsCloudInitMaterial{
					commonHostMaterial: commonHostMaterial{hostName: hostName, hostPrivateKey: values[0].(string), hostCertificate: values[1].(string)},
					deployPublicKey:    values[2].(string), valkeyPassword: values[3].(string), grafanaAdminPassword: values[4].(string), metricsBearerToken: values[5].(string), backupEnvironment: values[6].(string),
					postgresCACertificate: values[7].(string), postgresJobsCertificate: values[8].(string), postgresJobsPrivateKey: values[9].(string),
				})
			}).(pulumi.StringOutput)
		case "db-primary":
			userData[spec.name] = pulumi.All(
				hostIdentity.privateKey,
				hostIdentity.certificate,
				dataVolume.ID(),
				postgresEnvironment,
				postgresIdentity.caCertificate,
				postgresIdentity.serverCertificate,
				postgresIdentity.serverPrivateKey,
			).ApplyT(func(values []interface{}) (string, error) {
				return renderDatabaseCloudInit(databaseCloudInitMaterial{
					commonHostMaterial: commonHostMaterial{hostName: hostName, hostPrivateKey: values[0].(string), hostCertificate: values[1].(string)},
					volumeID:           string(values[2].(pulumi.ID)), postgresEnvironment: values[3].(string), postgresCACertificate: values[4].(string), postgresServerCertificate: values[5].(string), postgresServerPrivateKey: values[6].(string),
				})
			}).(pulumi.StringOutput)
		}
	}

	servers := make(map[string]*hcloud.Server, len(nodes))
	createServer := func(spec nodeSpec, dependencies []pulumi.Resource) (*hcloud.Server, error) {
		var firewall *hcloud.Firewall
		switch spec.role {
		case "app":
			firewall = appFirewall
		case "ops":
			firewall = opsFirewall
		default:
			firewall = databaseFirewall
		}
		args := standardServerArgs(spec, subnet, sshKeys, pulumi.IntArray{idToInt(firewall.ID())})
		args.UserData = userData[spec.name]
		args.DeleteProtection = pulumi.Bool(protected)
		args.RebuildProtection = pulumi.Bool(protected)
		if spec.role == "ops" {
			args.PublicNets = hcloud.ServerPublicNetArray{&hcloud.ServerPublicNetArgs{Ipv4: idToInt(opsPrimaryIP.ID()).ToIntPtrOutput(), Ipv4Enabled: pulumi.Bool(true), Ipv6Enabled: pulumi.Bool(false)}}
		} else {
			args.PublicNets = hcloud.ServerPublicNetArray{&hcloud.ServerPublicNetArgs{Ipv4Enabled: pulumi.Bool(false), Ipv6Enabled: pulumi.Bool(false)}}
		}
		if spec.role == "app" {
			args.PlacementGroupId = idToInt(appPlacement.ID()).ToIntPtrOutput()
		}
		resourceOptions := []pulumi.ResourceOption{
			opts,
			pulumi.DependsOn(append(dependencies, subnet, firewall)),
			pulumi.DeleteBeforeReplace(true),
			pulumi.ReplaceOnChanges([]string{"userData"}),
		}
		if protected {
			resourceOptions = append(resourceOptions, pulumi.Protect(true))
		}
		return hcloud.NewServer(ctx, spec.name, args, resourceOptions...)
	}

	var opsSpec nodeSpec
	for _, spec := range nodes {
		if spec.role == "ops" {
			opsSpec = spec
			break
		}
	}
	opsServer, err := createServer(opsSpec, []pulumi.Resource{opsPrimaryIP})
	if err != nil {
		return nil, err
	}
	servers[opsSpec.name] = opsServer
	networkRoute, err := hcloud.NewNetworkRoute(ctx, "private-egress-route", &hcloud.NetworkRouteArgs{
		NetworkId:   idToInt(network.ID()),
		Destination: pulumi.String("0.0.0.0/0"),
		Gateway:     pulumi.String(valkeyPrivateIP),
	}, opts, pulumi.DependsOn([]pulumi.Resource{subnet, opsServer}))
	if err != nil {
		return nil, err
	}
	for _, spec := range nodes {
		if spec.role == "ops" {
			continue
		}
		server, err := createServer(spec, []pulumi.Resource{networkRoute, opsServer})
		if err != nil {
			return nil, err
		}
		servers[spec.name] = server
	}

	_, err = hcloud.NewVolumeAttachment(ctx, "db-primary-data-attachment", &hcloud.VolumeAttachmentArgs{
		VolumeId:  idToInt(dataVolume.ID()),
		ServerId:  idToInt(servers["db-primary"].ID()),
		Automount: pulumi.Bool(false),
	}, opts)
	if err != nil {
		return nil, err
	}

	ctx.Export("deploySshPrivateKey", sshIdentity.deployPrivateKey)
	ctx.Export("sshHostCaPublicKey", sshIdentity.caPublicKey)
	ctx.Export("sshKnownHostsCertificateAuthority", sshIdentity.caPublicKey.ApplyT(func(key string) string {
		return "@cert-authority kamori-beta-ops,kamori-beta-app-1,kamori-beta-app-2,[kamori-beta-ops]:2022,[kamori-beta-app-1]:2022,[kamori-beta-app-2]:2022 " + key
	}).(pulumi.StringOutput))
	ctx.Export("grafanaAdminPassword", passwords.grafanaAdmin)
	ctx.Export("drBlobBucketConfiguredForOps", pulumi.String(drBucketName))
	return &hostResources{servers: servers, sshPKI: sshIdentity}, nil
}

func renderApplicationEnvironment(cfg *config.Config) pulumi.StringOutput {
	return pulumi.All(
		cfg.RequireSecret("databasePassword"),
		cfg.RequireSecret("valkeyPassword"),
		cfg.RequireSecret("jwtSecret"),
		cfg.RequireSecret("adminTotpKek"),
		cfg.RequireSecret("authTotpKek"),
		cfg.RequireSecret("b2RuntimeKeyId"),
		cfg.RequireSecret("b2RuntimeApplicationKey"),
		cfg.RequireSecret("metricsBearerToken"),
	).ApplyT(func(values []interface{}) string {
		return renderCloudEnv(cloudEnvSecrets{
			databasePassword: values[0].(string), valkeyPassword: values[1].(string), jwtSecret: values[2].(string), adminTotpKek: values[3].(string), authTotpKek: values[4].(string),
			objectStoreKeyID: values[5].(string), objectStoreSecretKey: values[6].(string), metricsBearerToken: values[7].(string),
		}, backblazeEndpoint, backblazeRegion, backblazePrimaryBucket)
	}).(pulumi.StringOutput)
}

func hostNodeSpecs(cfg *config.Config) []nodeSpec {
	nodes := []nodeSpec{
		{name: "app-1", role: "app", serverType: cfg.Get("appServerType"), location: "nbg1", privateIP: "10.42.0.11"},
		{name: "app-2", role: "app", serverType: cfg.Get("appServerType"), location: "fsn1", privateIP: "10.42.0.12"},
		{name: "db-primary", role: "db-primary", serverType: cfg.Get("dbServerType"), location: "nbg1", privateIP: databasePrimaryPrivateIP},
		{name: "ops", role: "ops", serverType: cfg.Get("opsServerType"), location: "hel1", privateIP: valkeyPrivateIP},
	}
	for index := range nodes {
		if nodes[index].serverType != "" {
			continue
		}
		switch nodes[index].role {
		case "app":
			nodes[index].serverType = defaultAppServerType
		case "ops":
			nodes[index].serverType = defaultOpsServerType
		default:
			nodes[index].serverType = defaultDatabaseServerType
		}
	}
	return nodes
}

func standardServerArgs(spec nodeSpec, subnet *hcloud.NetworkSubnet, sshKeys []string, firewallIDs pulumi.IntArray) *hcloud.ServerArgs {
	return &hcloud.ServerArgs{
		Name:                   pulumi.String("kamori-beta-" + spec.name),
		Image:                  pulumi.String("ubuntu-24.04"),
		ServerType:             pulumi.String(spec.serverType),
		Location:               pulumi.String(spec.location),
		SshKeys:                stringsToInputs(sshKeys),
		FirewallIds:            firewallIDs,
		Backups:                pulumi.Bool(true),
		Labels:                 commonLabels(spec.role),
		Networks:               hcloud.ServerNetworkTypeArray{&hcloud.ServerNetworkTypeArgs{SubnetId: subnet.ID(), Ip: pulumi.String(spec.privateIP), AliasIps: pulumi.StringArray{}}},
		ShutdownBeforeDeletion: pulumi.Bool(true),
	}
}
