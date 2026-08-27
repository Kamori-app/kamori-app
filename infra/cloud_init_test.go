package main

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"encoding/base64"
	"io"
	"strings"
	"testing"

	"gopkg.in/yaml.v3"
)

func decodeHostConfiguration(t *testing.T, encoded string) map[string]string {
	t.Helper()
	compressed, err := base64.StdEncoding.DecodeString(encoded)
	if err != nil {
		t.Fatal(err)
	}
	gzipReader, err := gzip.NewReader(bytes.NewReader(compressed))
	if err != nil {
		t.Fatal(err)
	}
	tarReader := tar.NewReader(gzipReader)
	files := make(map[string]string)
	for {
		header, err := tarReader.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			t.Fatal(err)
		}
		contents, err := io.ReadAll(tarReader)
		if err != nil {
			t.Fatal(err)
		}
		files[header.Name] = string(contents)
	}
	if err := gzipReader.Close(); err != nil {
		t.Fatal(err)
	}
	return files
}

func TestAutomatedCloudInitIsDeterministicAndBelowProviderLimit(t *testing.T) {
	common := commonHostMaterial{
		hostName:        "kamori-beta-app-1",
		hostPrivateKey:  "HOST PRIVATE KEY",
		hostPublicKey:   "ssh-ed25519 HOST PUBLIC KEY",
		hostCertificate: "HOST CERTIFICATE",
	}
	material := appCloudInitMaterial{
		commonHostMaterial:        common,
		deployPublicKey:           "ssh-ed25519 DEPLOY",
		cloudEnvironment:          "KAMORI_JWT_SECRET=secret\n",
		opaqueServerSetup:         "opaque",
		refreshRotationKey:        "rotation",
		postgresCACertificate:     "CA",
		postgresClientCertificate: "CLIENT CERT",
		postgresClientPrivateKey:  "CLIENT KEY",
	}
	first, err := renderAppCloudInit(material)
	if err != nil {
		t.Fatal(err)
	}
	second, err := renderAppCloudInit(material)
	if err != nil {
		t.Fatal(err)
	}
	if first != second {
		t.Fatal("cloud-init output changed without an input change")
	}
	if len(first) >= cloudInitMaximumBytes {
		t.Fatalf("cloud-init is %d bytes, provider limit is %d", len(first), cloudInitMaximumBytes)
	}
	var parsed map[string]interface{}
	if err := yaml.Unmarshal([]byte(first), &parsed); err != nil {
		t.Fatalf("cloud-init is not valid YAML: %v", err)
	}
	for _, required := range []string{"#cloud-config", "/usr/local/sbin/kamori-first-boot", "gzip+base64", "kamori-beta-app-1"} {
		if !strings.Contains(first, required) {
			t.Fatalf("cloud-init is missing %q", required)
		}
	}
}

func TestRotatableApplicationMaterialLivesOutsideImmutableCloudInit(t *testing.T) {
	t.Parallel()
	material := appCloudInitMaterial{
		commonHostMaterial: commonHostMaterial{
			hostName: "kamori-beta-app-1", hostPrivateKey: "HOST PRIVATE", hostPublicKey: "ssh-ed25519 HOST", hostCertificate: "HOST CERT", configPublicKey: "ssh-ed25519 CONFIG",
		},
		deployPublicKey: "ssh-ed25519 DEPLOY", cloudEnvironment: "KAMORI_JWT_SECRET=runtime-secret\n", opaqueServerSetup: "opaque-secret", refreshRotationKey: "rotation-secret",
		postgresCACertificate: "POSTGRES CA", postgresClientCertificate: "POSTGRES CLIENT", postgresClientPrivateKey: "POSTGRES PRIVATE",
	}
	cloudInit, err := renderAppCloudInit(material)
	if err != nil {
		t.Fatal(err)
	}
	bootstrapFiles := decodeCloudInitFiles(t, cloudInit)
	for _, forbidden := range []string{
		"/etc/kamori/cloud.env",
		"/etc/kamori/secrets/opaque-server-setup",
		"/etc/kamori/postgres-client.key",
		"/etc/ssh/ssh_host_ed25519_key-cert.pub",
	} {
		if _, ok := bootstrapFiles[forbidden]; ok {
			t.Fatalf("rotatable file %s leaked into immutable cloud-init", forbidden)
		}
	}

	configuration, err := renderAppHostConfiguration(material)
	if err != nil {
		t.Fatal(err)
	}
	configurationFiles := decodeHostConfiguration(t, configuration)
	for path, expected := range map[string]string{
		".kamori-role":                                 "app\n",
		"root/etc/kamori/cloud.env":                    "KAMORI_JWT_SECRET=runtime-secret\n",
		"root/etc/kamori/secrets/opaque-server-setup":  "opaque-secret",
		"root/etc/kamori/secrets/refresh-rotation-key": "rotation-secret",
		"root/etc/kamori/postgres-client.key":          "POSTGRES PRIVATE",
		"root/etc/ssh/ssh_host_ed25519_key-cert.pub":   "HOST CERT",
	} {
		if got := configurationFiles[path]; got != expected {
			t.Fatalf("host configuration %s = %q, want %q", path, got, expected)
		}
	}
}

func TestEveryRoleCloudInitIsValidAndBelowProviderLimit(t *testing.T) {
	common := commonHostMaterial{hostName: "kamori-beta-test", hostPrivateKey: "HOST KEY", hostPublicKey: "ssh-ed25519 HOST PUBLIC KEY", hostCertificate: "HOST CERT"}
	documents := map[string]struct {
		value string
		err   error
	}{
		"ops": func() struct {
			value string
			err   error
		} {
			value, err := renderOpsCloudInit(opsCloudInitMaterial{
				commonHostMaterial: common, deployPublicKey: "ssh-ed25519 DEPLOY", valkeyPassword: "valkey", grafanaAdminPassword: "grafana", metricsBearerToken: "metrics",
				backupEnvironment: "PRIMARY_S3_KEY_ID=read\n", postgresCACertificate: "CA", postgresJobsCertificate: "JOBS CERT", postgresJobsPrivateKey: "JOBS KEY",
			})
			return struct {
				value string
				err   error
			}{value, err}
		}(),
		"database": func() struct {
			value string
			err   error
		} {
			value, err := renderDatabaseCloudInit(databaseCloudInitMaterial{
				commonHostMaterial: common, volumeID: "123", postgresEnvironment: "POSTGRES_VERSION=16\n", postgresCACertificate: "CA", postgresServerCertificate: "SERVER CERT", postgresServerPrivateKey: "SERVER KEY",
			})
			return struct {
				value string
				err   error
			}{value, err}
		}(),
	}
	for role, document := range documents {
		if document.err != nil {
			t.Fatalf("render %s cloud-init: %v", role, document.err)
		}
		if len(document.value) >= cloudInitMaximumBytes {
			t.Fatalf("%s cloud-init is %d bytes, provider limit is %d", role, len(document.value), cloudInitMaximumBytes)
		}
		var parsed map[string]interface{}
		if err := yaml.Unmarshal([]byte(document.value), &parsed); err != nil {
			t.Fatalf("%s cloud-init is not valid YAML: %v", role, err)
		}
	}
}

func TestPrivateHostEgressConfiguresProviderDNSBeforePackageInstallation(t *testing.T) {
	t.Parallel()

	script := commonFirstBoot("curl", true)
	routeSetup := strings.Index(script, "/usr/local/sbin/kamori-private-default-route")
	packageInstall := strings.Index(script, "for attempt in $(seq 1 120); do")
	if routeSetup < 0 {
		t.Fatal("private first-boot script does not configure egress before package installation")
	}
	if packageInstall < 0 || routeSetup >= packageInstall {
		t.Fatal("private egress must be configured before package installation")
	}

	document, err := renderCloudInit("app", commonHostMaterial{
		hostName:        "kamori-beta-app-1",
		hostPrivateKey:  "HOST KEY",
		hostPublicKey:   "ssh-ed25519 HOST PUBLIC KEY",
		hostCertificate: "HOST CERT",
	}, nil, script)
	if err != nil {
		t.Fatal(err)
	}

	files := decodeCloudInitFiles(t, document)
	routeScript, ok := files["/usr/local/sbin/kamori-private-default-route"]
	if !ok {
		t.Fatal("private cloud-init is missing its egress setup script")
	}
	for _, command := range []string{
		`ip route replace default via 10.42.0.1 dev "$private_interface" onlink`,
		`resolvectl dns "$private_interface" 185.12.64.1 185.12.64.2`,
		`resolvectl domain "$private_interface" '~.'`,
		`resolvectl default-route "$private_interface" yes`,
	} {
		if !strings.Contains(routeScript, command) {
			t.Fatalf("private egress setup is missing %q", command)
		}
	}
}

func TestPrivateHostConfigurationPersistsAndReappliesEgress(t *testing.T) {
	t.Parallel()

	material := appCloudInitMaterial{
		commonHostMaterial: commonHostMaterial{
			hostName: "kamori-beta-app-1", hostCertificate: "HOST CERT", configPublicKey: "ssh-ed25519 CONFIG",
		},
		deployPublicKey: "ssh-ed25519 DEPLOY", cloudEnvironment: "KAMORI_JWT_SECRET=secret\n", opaqueServerSetup: "opaque", refreshRotationKey: "rotation",
		postgresCACertificate: "CA", postgresClientCertificate: "CLIENT CERT", postgresClientPrivateKey: "CLIENT KEY",
	}
	configuration, err := renderAppHostConfiguration(material)
	if err != nil {
		t.Fatal(err)
	}
	files := decodeHostConfiguration(t, configuration)
	resolver := files["root/etc/systemd/resolved.conf.d/60-kamori-private-egress.conf"]
	if resolver != "[Resolve]\nDNS=185.12.64.1 185.12.64.2\n" {
		t.Fatalf("private resolver configuration = %q", resolver)
	}
	if _, ok := files["root/usr/local/sbin/kamori-repair-egress"]; !ok {
		t.Fatal("host configuration is missing the restricted egress repair entrypoint")
	}
	if _, ok := files["root/usr/local/sbin/kamori-install-host-config"]; !ok {
		t.Fatal("host configuration is missing the atomic configuration installer")
	}
	if _, ok := files["root/usr/local/sbin/kamori-apply-host-config"]; ok {
		t.Fatal("host configuration must not overwrite the legacy running installer")
	}
	for _, required := range []string{
		"/usr/local/sbin/kamori-install-host-config app",
		"/usr/local/sbin/kamori-repair-egress app",
	} {
		if !strings.Contains(files["root/etc/sudoers.d/kamori-configure"], required) {
			t.Fatalf("configuration sudoers is missing exact command %q", required)
		}
	}

	installScript, err := deploymentAsset("host-config/kamori-install-host-config")
	if err != nil {
		t.Fatal(err)
	}
	for _, required := range []string{
		"/usr/local/sbin/kamori-repair-egress app",
		"/usr/local/sbin/kamori-repair-egress ops",
		"/usr/local/sbin/kamori-repair-egress db-primary",
	} {
		if !strings.Contains(installScript, required) {
			t.Fatalf("host configuration does not delegate to %q", required)
		}
	}

	repairScript, err := deploymentAsset("host-config/kamori-repair-egress")
	if err != nil {
		t.Fatal(err)
	}
	for _, required := range []string{
		"systemctl restart systemd-resolved.service",
		"systemctl restart kamori-private-egress.service",
		"systemctl restart kamori-nat-gateway.service",
		"resolvectl flush-caches",
	} {
		if !strings.Contains(repairScript, required) {
			t.Fatalf("egress repair is missing recovery command %q", required)
		}
	}
	for _, forbidden := range []string{
		"docker compose",
		"bootstrap-primary",
		"pgbackrest",
		"curl ",
		"/health",
		"resolvectl query",
	} {
		if strings.Contains(repairScript, forbidden) {
			t.Fatalf("restricted egress repair must not contain %q", forbidden)
		}
	}
}

func TestConfigurationDispatchAllowsOnlyExactReviewedCommands(t *testing.T) {
	t.Parallel()

	dispatch, err := deploymentAsset("host-config/kamori-config-dispatch")
	if err != nil {
		t.Fatal(err)
	}
	for _, required := range []string{
		`"apply ${role}")`,
		`"repair-egress ${role}")`,
		`exec sudo /usr/local/sbin/kamori-install-host-config "$role"`,
		`exec sudo /usr/local/sbin/kamori-repair-egress "$role"`,
	} {
		if !strings.Contains(dispatch, required) {
			t.Fatalf("configuration dispatch is missing %q", required)
		}
	}
}

func TestUnchangedHostConfigurationSkipsRoleActivation(t *testing.T) {
	t.Parallel()

	installer, err := deploymentAsset("host-config/kamori-install-host-config")
	if err != nil {
		t.Fatal(err)
	}
	for _, required := range []string{
		`configuration_marker="/var/lib/kamori/host-configuration-${role}.sha256"`,
		`configuration_fingerprint=$(sha256sum "$archive"`,
		`if [[ "$installed_configuration_fingerprint" == "$configuration_fingerprint" ]]`,
		`mv "$marker_temporary" "$configuration_marker"`,
	} {
		if !strings.Contains(installer, required) {
			t.Fatalf("host configuration installer is missing %q", required)
		}
	}
	skip := strings.Index(installer, `if [[ "$installed_configuration_fingerprint" == "$configuration_fingerprint" ]]`)
	installFiles := strings.Index(installer, `while IFS= read -r -d '' source; do`)
	writeMarker := strings.Index(installer, `mv "$marker_temporary" "$configuration_marker"`)
	if skip < 0 || installFiles < 0 || writeMarker < 0 || skip >= installFiles || installFiles >= writeMarker {
		t.Fatal("configuration must skip before activation and record its fingerprint only after activation")
	}
}

func TestHostConfigurationFilesAreInstalledAtomically(t *testing.T) {
	t.Parallel()

	installer, err := deploymentAsset("host-config/kamori-install-host-config")
	if err != nil {
		t.Fatal(err)
	}
	for _, required := range []string{
		`install_temporary=$(mktemp "$destination_directory/.kamori-host-config.XXXXXX")`,
		`chmod --reference="$source" "$install_temporary"`,
		`mv -fT "$install_temporary" "$destination"`,
		"rm -f /usr/local/sbin/kamori-apply-host-config",
	} {
		if !strings.Contains(installer, required) {
			t.Fatalf("atomic host configuration installer is missing %q", required)
		}
	}
	if strings.Contains(installer, `cp -a "$work_dir/root/." /`) {
		t.Fatal("host configuration must not overwrite a running shell script in place")
	}
}

func TestPostgresRepositoryCheckRunsOnlyForChangedBackupConfiguration(t *testing.T) {
	t.Parallel()

	bootstrap, err := deploymentAsset("postgres/bootstrap-primary")
	if err != nil {
		t.Fatal(err)
	}
	for _, required := range []string{
		"backup_config_marker=/var/lib/kamori/pgbackrest-configuration.sha256",
		"backup_config_fingerprint=$(sha256sum /etc/pgbackrest/pgbackrest.conf",
		`if [[ "$installed_backup_config_fingerprint" != "$backup_config_fingerprint" ]]`,
		`mv "$temporary_marker" "$backup_config_marker"`,
	} {
		if !strings.Contains(bootstrap, required) {
			t.Fatalf("PostgreSQL bootstrap is missing %q", required)
		}
	}
	if count := strings.Count(bootstrap, "pgbackrest --stanza=kamori check"); count != 1 {
		t.Fatalf("PostgreSQL bootstrap contains %d repository checks, want one guarded check", count)
	}
}

func TestRegistryOperationsHaveBoundedRetries(t *testing.T) {
	t.Parallel()

	login, err := deploymentAsset("cloud-server/kamori-registry-login")
	if err != nil {
		t.Fatal(err)
	}
	deploy, err := deploymentAsset("cloud-server/deploy-cloud-server")
	if err != nil {
		t.Fatal(err)
	}
	for name, script := range map[string]string{"login": login, "deploy": deploy} {
		if !strings.Contains(script, "for attempt in $(seq 1 5); do") {
			t.Fatalf("%s script is missing its bounded registry retry", name)
		}
		if !strings.Contains(script, `if [[ "$attempt" == 5 ]]`) {
			t.Fatalf("%s script does not stop after the bounded retry count", name)
		}
	}
}

func TestContainerHostsDisableCloudInitNetworkHotplug(t *testing.T) {
	t.Parallel()

	common := commonHostMaterial{hostName: "kamori-beta-test", hostPrivateKey: "HOST KEY", hostPublicKey: "ssh-ed25519 HOST PUBLIC KEY", hostCertificate: "HOST CERT"}
	app, err := renderAppCloudInit(appCloudInitMaterial{
		commonHostMaterial: common, deployPublicKey: "ssh-ed25519 DEPLOY", cloudEnvironment: "KAMORI_JWT_SECRET=secret\n", opaqueServerSetup: "opaque", refreshRotationKey: "rotation",
		postgresCACertificate: "CA", postgresClientCertificate: "CLIENT CERT", postgresClientPrivateKey: "CLIENT KEY",
	})
	if err != nil {
		t.Fatal(err)
	}
	ops, err := renderOpsCloudInit(opsCloudInitMaterial{
		commonHostMaterial: common, deployPublicKey: "ssh-ed25519 DEPLOY", valkeyPassword: "valkey", grafanaAdminPassword: "grafana", metricsBearerToken: "metrics",
		backupEnvironment: "PRIMARY_S3_KEY_ID=read\n", postgresCACertificate: "CA", postgresJobsCertificate: "JOBS CERT", postgresJobsPrivateKey: "JOBS KEY",
	})
	if err != nil {
		t.Fatal(err)
	}
	database, err := renderDatabaseCloudInit(databaseCloudInitMaterial{
		commonHostMaterial: common, volumeID: "123", postgresEnvironment: "POSTGRES_VERSION=16\n", postgresCACertificate: "CA", postgresServerCertificate: "SERVER CERT", postgresServerPrivateKey: "SERVER KEY",
	})
	if err != nil {
		t.Fatal(err)
	}

	const maskCommand = "systemctl mask cloud-init-hotplugd.socket cloud-init-hotplugd.service"
	const idempotentReset = "systemctl reset-failed cloud-init-hotplugd.service || true"
	for role, document := range map[string]string{"app": app, "ops": ops} {
		firstBoot := decodeCloudInitFiles(t, document)["/usr/local/sbin/kamori-first-boot"]
		if !strings.Contains(firstBoot, maskCommand) {
			t.Fatalf("%s container host does not disable cloud-init network hotplug", role)
		}
		if !strings.Contains(firstBoot, idempotentReset) {
			t.Fatalf("%s container host does not reset a missing hotplug failure idempotently", role)
		}
	}
	databaseFirstBoot := decodeCloudInitFiles(t, database)["/usr/local/sbin/kamori-first-boot"]
	if strings.Contains(databaseFirstBoot, maskCommand) {
		t.Fatal("database host must retain cloud-init network hotplug support")
	}
}

func TestMigrationUsesTheServiceComposeEnvironment(t *testing.T) {
	t.Parallel()

	script, err := deploymentAsset("cloud-server/deploy-cloud-server")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(script, "/usr/bin/docker run --rm") {
		t.Fatal("migration must not use docker run because its env-file parser differs from Compose")
	}
	for _, required := range []string{
		"--file /opt/kamori/release/compose.yaml",
		`--env-file "$temporary_release"`,
		"run --rm --no-deps migration migrate",
	} {
		if !strings.Contains(script, required) {
			t.Fatalf("migration command is missing %q", required)
		}
	}
	release := strings.Index(script, "temporary_release=$(mktemp")
	migration := strings.Index(script, "run --rm --no-deps migration migrate")
	if release < 0 || migration < 0 || release >= migration {
		t.Fatal("immutable release environment must be rendered before the migration starts")
	}
}

func TestMigrationServiceDoesNotClaimTheRuntimeAddress(t *testing.T) {
	t.Parallel()

	compose, err := deploymentAsset("cloud-server/compose.yaml")
	if err != nil {
		t.Fatal(err)
	}
	migrationStart := strings.Index(compose, "\n  migration:\n")
	webStart := strings.Index(compose, "\n  web:\n")
	if migrationStart < 0 || webStart <= migrationStart {
		t.Fatal("compose file is missing the isolated migration service")
	}
	migration := compose[migrationStart:webStart]
	for _, required := range []string{"profiles:", "- migration", "- kamori_internal"} {
		if !strings.Contains(migration, required) {
			t.Fatalf("migration service is missing %q", required)
		}
	}
	if strings.Contains(migration, "ipv4_address:") || strings.Contains(migration, "ports:") {
		t.Fatal("migration service must use a dynamic private address and expose no host ports")
	}
	var parsed struct {
		Services map[string]struct {
			Image       string   `yaml:"image"`
			Environment []string `yaml:"env_file"`
			Volumes     []string `yaml:"volumes"`
			ReadOnly    bool     `yaml:"read_only"`
			Profiles    []string `yaml:"profiles"`
		} `yaml:"services"`
	}
	if err := yaml.Unmarshal([]byte(compose), &parsed); err != nil {
		t.Fatalf("cloud compose is not valid YAML: %v", err)
	}
	cloud, cloudExists := parsed.Services["cloud"]
	migrationService, migrationExists := parsed.Services["migration"]
	if !cloudExists || !migrationExists {
		t.Fatal("cloud compose must define runtime and migration services")
	}
	if migrationService.Image != cloud.Image || len(migrationService.Environment) == 0 ||
		len(migrationService.Volumes) != len(cloud.Volumes) || !migrationService.ReadOnly {
		t.Fatal("migration service must inherit the cloud image, environment, secrets, and hardening")
	}
	if len(migrationService.Profiles) != 1 || migrationService.Profiles[0] != "migration" {
		t.Fatal("migration service must remain excluded from the normal runtime profile")
	}
}

func decodeCloudInitFiles(t *testing.T, document string) map[string]string {
	t.Helper()

	var parsed struct {
		WriteFiles []struct {
			Path     string `yaml:"path"`
			Encoding string `yaml:"encoding"`
			Content  string `yaml:"content"`
		} `yaml:"write_files"`
	}
	if err := yaml.Unmarshal([]byte(document), &parsed); err != nil {
		t.Fatalf("cloud-init is not valid YAML: %v", err)
	}

	files := make(map[string]string, len(parsed.WriteFiles))
	for _, file := range parsed.WriteFiles {
		if file.Encoding != "gzip+base64" {
			t.Fatalf("%s uses unexpected encoding %q", file.Path, file.Encoding)
		}
		compressed, err := base64.StdEncoding.DecodeString(file.Content)
		if err != nil {
			t.Fatalf("decode %s: %v", file.Path, err)
		}
		reader, err := gzip.NewReader(bytes.NewReader(compressed))
		if err != nil {
			t.Fatalf("decompress %s: %v", file.Path, err)
		}
		contents, err := io.ReadAll(reader)
		if err != nil {
			t.Fatalf("read %s: %v", file.Path, err)
		}
		if err := reader.Close(); err != nil {
			t.Fatalf("close %s reader: %v", file.Path, err)
		}
		files[file.Path] = string(contents)
	}
	return files
}

func TestFirstBootPinsSSHSocketBeforePackageInstallation(t *testing.T) {
	t.Parallel()

	script := commonFirstBoot("curl", false)
	commands := []string{
		"install -d -o root -g root -m 0700 /var/lib/kamori/bootstrap",
		"install -o root -g root -m 0600 /var/lib/kamori/bootstrap/ssh_host_ed25519_key /etc/ssh/ssh_host_ed25519_key",
		"install -o root -g root -m 0644 /var/lib/kamori/bootstrap/ssh_host_ed25519_key.pub /etc/ssh/ssh_host_ed25519_key.pub",
		"install -o deploy -g deploy -m 0600 /etc/kamori/config-authorized-key /home/deploy/.ssh/authorized_keys",
		"install -d -o root -g root -m 0755 /run/sshd",
		"sshd -t",
		"systemctl stop ssh.service",
		"systemctl daemon-reload",
		"systemctl enable ssh.socket",
		"systemctl restart ssh.socket",
	}
	previous := -1
	for _, command := range commands {
		position := strings.Index(script, command)
		if position < 0 {
			t.Fatalf("first-boot script is missing %q", command)
		}
		if position <= previous {
			t.Fatalf("first-boot SSH transition is out of order at %q", command)
		}
		previous = position
	}

	packageInstall := strings.Index(script, "for attempt in $(seq 1 120); do")
	if packageInstall < 0 {
		t.Fatal("first-boot script is missing the package installation loop")
	}
	if previous >= packageInstall {
		t.Fatal("SSH must move to its configured port before package installation")
	}
	for _, forbidden := range []string{
		"systemctl disable --now ssh.socket",
		"systemctl enable ssh.service",
		"systemctl restart ssh.service",
	} {
		if strings.Contains(script, forbidden) {
			t.Fatalf("first-boot script must not switch away from socket activation: found %q", forbidden)
		}
	}
}

func TestRenderedCloudInitReplacesDefaultSSHSocketListener(t *testing.T) {
	t.Parallel()

	document, err := renderCloudInit("test", commonHostMaterial{
		hostName:        "kamori-beta-test",
		hostPrivateKey:  "HOST KEY",
		hostPublicKey:   "ssh-ed25519 HOST PUBLIC KEY",
		hostCertificate: "HOST CERT",
	}, nil, commonFirstBoot("curl", false))
	if err != nil {
		t.Fatal(err)
	}

	var parsed struct {
		WriteFiles []struct {
			Path     string `yaml:"path"`
			Encoding string `yaml:"encoding"`
			Content  string `yaml:"content"`
		} `yaml:"write_files"`
	}
	if err := yaml.Unmarshal([]byte(document), &parsed); err != nil {
		t.Fatalf("cloud-init is not valid YAML: %v", err)
	}

	const socketDropIn = "/etc/systemd/system/ssh.socket.d/60-kamori-listen.conf"
	for _, file := range parsed.WriteFiles {
		if file.Path != socketDropIn {
			continue
		}
		if file.Encoding != "gzip+base64" {
			t.Fatalf("SSH socket drop-in uses unexpected encoding %q", file.Encoding)
		}
		compressed, err := base64.StdEncoding.DecodeString(file.Content)
		if err != nil {
			t.Fatalf("decode SSH socket drop-in: %v", err)
		}
		reader, err := gzip.NewReader(bytes.NewReader(compressed))
		if err != nil {
			t.Fatalf("decompress SSH socket drop-in: %v", err)
		}
		contents, err := io.ReadAll(reader)
		if err != nil {
			t.Fatalf("read SSH socket drop-in: %v", err)
		}
		if err := reader.Close(); err != nil {
			t.Fatalf("close SSH socket drop-in reader: %v", err)
		}
		if got, want := string(contents), "[Socket]\nListenStream=\nListenStream=2022\n"; got != want {
			t.Fatalf("SSH socket drop-in = %q, want %q", got, want)
		}
		return
	}
	t.Fatalf("cloud-init is missing %s", socketDropIn)
}

func TestRenderedCloudInitStagesRawSSHHostIdentityOutsideEtcSSH(t *testing.T) {
	t.Parallel()

	document, err := renderCloudInit("test", commonHostMaterial{
		hostName:        "kamori-beta-test",
		hostPrivateKey:  "HOST PRIVATE KEY",
		hostPublicKey:   "ssh-ed25519 HOST PUBLIC KEY",
		hostCertificate: "HOST CERTIFICATE",
	}, nil, commonFirstBoot("curl", false))
	if err != nil {
		t.Fatal(err)
	}

	var parsed struct {
		WriteFiles []struct {
			Path        string `yaml:"path"`
			Permissions string `yaml:"permissions"`
			Encoding    string `yaml:"encoding"`
			Content     string `yaml:"content"`
		} `yaml:"write_files"`
	}
	if err := yaml.Unmarshal([]byte(document), &parsed); err != nil {
		t.Fatalf("cloud-init is not valid YAML: %v", err)
	}

	expected := map[string]struct {
		permissions string
		content     string
	}{
		"/var/lib/kamori/bootstrap/ssh_host_ed25519_key":     {permissions: "0600", content: "HOST PRIVATE KEY"},
		"/var/lib/kamori/bootstrap/ssh_host_ed25519_key.pub": {permissions: "0644", content: "ssh-ed25519 HOST PUBLIC KEY"},
	}

	for _, file := range parsed.WriteFiles {
		if strings.HasPrefix(file.Path, "/etc/ssh/ssh_host_") {
			t.Fatalf("host identity must not be written before cloud-init cc_ssh: %s", file.Path)
		}
		want, ok := expected[file.Path]
		if !ok {
			continue
		}
		if file.Permissions != want.permissions {
			t.Fatalf("%s permissions = %q, want %q", file.Path, file.Permissions, want.permissions)
		}
		if file.Encoding != "gzip+base64" {
			t.Fatalf("%s uses unexpected encoding %q", file.Path, file.Encoding)
		}
		compressed, err := base64.StdEncoding.DecodeString(file.Content)
		if err != nil {
			t.Fatalf("decode %s: %v", file.Path, err)
		}
		reader, err := gzip.NewReader(bytes.NewReader(compressed))
		if err != nil {
			t.Fatalf("decompress %s: %v", file.Path, err)
		}
		contents, err := io.ReadAll(reader)
		if err != nil {
			t.Fatalf("read %s: %v", file.Path, err)
		}
		if err := reader.Close(); err != nil {
			t.Fatalf("close %s reader: %v", file.Path, err)
		}
		if got := string(contents); got != want.content {
			t.Fatalf("%s content = %q, want %q", file.Path, got, want.content)
		}
		delete(expected, file.Path)
	}
	if len(expected) != 0 {
		for path := range expected {
			t.Errorf("cloud-init is missing %s", path)
		}
	}
}

func TestRenderedHostEnvironmentsKeepExternalCredentialsScoped(t *testing.T) {
	postgres := renderPostgresEnvironment("app", "jobs", "postgres-key", "postgres-secret", "cipher")
	for _, expected := range []string{
		"PGBACKREST_S3_BUCKET=kamori-production-postgres",
		`PGBACKREST_S3_KEY_ID="postgres-key"`,
		"POSTGRES_DATA_DIR=/srv/kamori-postgres/postgresql/16/main",
	} {
		if !strings.Contains(postgres, expected) {
			t.Fatalf("PostgreSQL environment is missing %q", expected)
		}
	}

	backup := renderBackupEnvironment("read-key", "read-secret", "dr-key", "dr-secret", "jobs", "kamori-app-production-dr")
	for _, expected := range []string{
		"PRIMARY_S3_BUCKET=kamori-production-primary",
		`PRIMARY_S3_KEY_ID="read-key"`,
		"DR_S3_BUCKET=kamori-app-production-dr",
		"postgres://kamori_jobs:jobs@10.42.0.21:5432/kamori",
	} {
		if !strings.Contains(backup, expected) {
			t.Fatalf("backup environment is missing %q", expected)
		}
	}
}

func TestOpsComposeUsesPublishedGrafanaRepository(t *testing.T) {
	t.Parallel()

	compose, err := deploymentAsset("ops/compose.yaml")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(compose, "grafana/grafana-oss:") {
		t.Fatal("ops compose uses the retired grafana-oss image repository")
	}
	if !strings.Contains(compose, "image: grafana/grafana:13.1.3") {
		t.Fatal("ops compose is missing the tested Grafana image pin")
	}
}

func TestOpsComposeRunsValkeyAsItsImageUser(t *testing.T) {
	t.Parallel()

	compose, err := deploymentAsset("ops/compose.yaml")
	if err != nil {
		t.Fatal(err)
	}
	var parsed struct {
		Services map[string]struct {
			Image    string   `yaml:"image"`
			User     string   `yaml:"user"`
			ReadOnly bool     `yaml:"read_only"`
			Tmpfs    []string `yaml:"tmpfs"`
		} `yaml:"services"`
	}
	if err := yaml.Unmarshal([]byte(compose), &parsed); err != nil {
		t.Fatalf("ops compose is not valid YAML: %v", err)
	}
	valkey, ok := parsed.Services["valkey"]
	if !ok {
		t.Fatal("ops compose is missing the Valkey service")
	}
	if valkey.Image != "valkey/valkey:9.0.3-alpine" {
		t.Fatalf("Valkey image = %q, want the tested 9.0.3 Alpine image", valkey.Image)
	}
	if valkey.User != "999:1000" {
		t.Fatalf("Valkey user = %q, want the pinned image's valkey UID/GID", valkey.User)
	}
	if !valkey.ReadOnly {
		t.Fatal("Valkey root filesystem must remain read-only")
	}
	if len(valkey.Tmpfs) != 1 || !strings.HasPrefix(valkey.Tmpfs[0], "/data:") {
		t.Fatalf("Valkey tmpfs = %v, want a writable ephemeral /data mount", valkey.Tmpfs)
	}
}

func TestEdgeImageDropsTheUnneededPrivilegedPortCapability(t *testing.T) {
	t.Parallel()

	dockerfile, err := deploymentAsset("edge/Dockerfile")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(dockerfile, "setcap -r /usr/bin/caddy") {
		t.Fatal("edge image must remove Caddy's privileged-port file capability")
	}
	if !strings.Contains(dockerfile, `test -z "$(getcap /usr/bin/caddy)"`) {
		t.Fatal("edge image build must verify that Caddy has no remaining file capabilities")
	}
}

func TestEdgePreservesApplicationGeneratedCSP(t *testing.T) {
	t.Parallel()

	caddyfile, err := deploymentAsset("edge/Caddyfile")
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(strings.ToLower(caddyfile), "content-security-policy") {
		t.Fatal("edge must preserve SvelteKit's nonce-bearing CSP instead of replacing it")
	}
}
