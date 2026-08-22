package main

import (
	"bytes"
	"compress/gzip"
	"encoding/base64"
	"io"
	"strings"
	"testing"

	"gopkg.in/yaml.v3"
)

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
		"install -o root -g root -m 0644 /var/lib/kamori/bootstrap/ssh_host_ed25519_key-cert.pub /etc/ssh/ssh_host_ed25519_key-cert.pub",
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

func TestRenderedCloudInitStagesCompleteSSHHostIdentityOutsideEtcSSH(t *testing.T) {
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
		"/var/lib/kamori/bootstrap/ssh_host_ed25519_key":          {permissions: "0600", content: "HOST PRIVATE KEY"},
		"/var/lib/kamori/bootstrap/ssh_host_ed25519_key.pub":      {permissions: "0644", content: "ssh-ed25519 HOST PUBLIC KEY"},
		"/var/lib/kamori/bootstrap/ssh_host_ed25519_key-cert.pub": {permissions: "0644", content: "HOST CERTIFICATE"},
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
