package main

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"encoding/base64"
	"fmt"
	"io"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
)

const cloudInitMaximumBytes = 32 * 1024

type cloudInitFile struct {
	path        string
	owner       string
	permissions string
	content     string
}

type commonHostMaterial struct {
	hostName        string
	hostPrivateKey  string
	hostPublicKey   string
	hostCertificate string
	configPublicKey string
}

type appCloudInitMaterial struct {
	commonHostMaterial
	deployPublicKey           string
	cloudEnvironment          string
	opaqueServerSetup         string
	refreshRotationKey        string
	postgresCACertificate     string
	postgresClientCertificate string
	postgresClientPrivateKey  string
}

type opsCloudInitMaterial struct {
	commonHostMaterial
	deployPublicKey         string
	valkeyPassword          string
	grafanaAdminPassword    string
	metricsBearerToken      string
	backupEnvironment       string
	postgresCACertificate   string
	postgresJobsCertificate string
	postgresJobsPrivateKey  string
}

type databaseCloudInitMaterial struct {
	commonHostMaterial
	volumeID                  string
	postgresEnvironment       string
	postgresCACertificate     string
	postgresServerCertificate string
	postgresServerPrivateKey  string
}

func deploymentAsset(relativePath string) (string, error) {
	contents, err := os.ReadFile(filepath.Join("..", "deploy", filepath.Clean(relativePath)))
	if err != nil {
		return "", fmt.Errorf("read deployment asset %s: %w", relativePath, err)
	}
	return string(contents), nil
}

func deploymentFiles(mapping map[string]string) ([]cloudInitFile, error) {
	files := make([]cloudInitFile, 0, len(mapping))
	destinations := make([]string, 0, len(mapping))
	for destination := range mapping {
		destinations = append(destinations, destination)
	}
	sort.Strings(destinations)
	for _, destination := range destinations {
		source := mapping[destination]
		contents, err := deploymentAsset(source)
		if err != nil {
			return nil, err
		}
		permissions := "0644"
		if !strings.HasSuffix(source, ".yaml") && !strings.HasSuffix(source, ".yml") && !strings.HasSuffix(source, ".service") && !strings.HasSuffix(source, ".timer") {
			permissions = "0755"
		}
		files = append(files, cloudInitFile{
			path:        destination,
			owner:       "root:root",
			permissions: permissions,
			content:     contents,
		})
	}
	return files, nil
}

func compressedBase64(value string) (string, error) {
	var compressed bytes.Buffer
	encoder := gzip.NewWriter(&compressed)
	if _, err := encoder.Write([]byte(value)); err != nil {
		return "", err
	}
	if err := encoder.Close(); err != nil {
		return "", err
	}
	return base64.StdEncoding.EncodeToString(compressed.Bytes()), nil
}

func renderHostConfiguration(role string, files []cloudInitFile) (string, error) {
	var compressed bytes.Buffer
	gzipWriter := gzip.NewWriter(&compressed)
	tarWriter := tar.NewWriter(gzipWriter)
	writeEntry := func(name string, mode int64, contents string) error {
		header := &tar.Header{
			Name: name,
			Mode: mode,
			Size: int64(len(contents)),
		}
		if err := tarWriter.WriteHeader(header); err != nil {
			return err
		}
		_, err := io.WriteString(tarWriter, contents)
		return err
	}
	if err := writeEntry(".kamori-role", 0o600, role+"\n"); err != nil {
		return "", err
	}
	for _, file := range files {
		mode, err := strconv.ParseInt(file.permissions, 8, 64)
		if err != nil {
			return "", fmt.Errorf("parse mode for %s: %w", file.path, err)
		}
		name := "root/" + strings.TrimPrefix(filepath.Clean(file.path), "/")
		if err := writeEntry(name, mode, file.content); err != nil {
			return "", fmt.Errorf("archive host configuration file %s: %w", file.path, err)
		}
	}
	if err := tarWriter.Close(); err != nil {
		return "", err
	}
	if err := gzipWriter.Close(); err != nil {
		return "", err
	}
	return base64.StdEncoding.EncodeToString(compressed.Bytes()), nil
}

func configAuthorizedKey(role, publicKey string) string {
	options := fmt.Sprintf(`command="/usr/local/sbin/kamori-config-dispatch %s",restrict`, role)
	if role == "ops" {
		options += `,port-forwarding,permitopen="10.42.0.11:2022",permitopen="10.42.0.12:2022",permitopen="10.42.0.21:2022"`
	}
	return options + " " + strings.TrimSpace(publicKey) + "\n"
}

func releaseAuthorizedKey(role, publicKey string) string {
	if strings.TrimSpace(publicKey) == "" {
		return ""
	}
	if role == "ops" {
		return `command="/bin/false",restrict,port-forwarding,permitopen="10.42.0.11:2022",permitopen="10.42.0.12:2022" ` + strings.TrimSpace(publicKey) + "\n"
	}
	return "restrict " + strings.TrimSpace(publicKey) + "\n"
}

func configurationSudoers(role string) string {
	return fmt.Sprintf(`deploy ALL=(root) NOPASSWD: /usr/local/sbin/kamori-apply-host-config %s
deploy ALL=(root) NOPASSWD: /usr/local/sbin/kamori-repair-egress %s
`, role, role)
}

func renderCloudInit(role string, common commonHostMaterial, files []cloudInitFile, firstBootScript string) (string, error) {
	applyHostConfig, err := deploymentAsset("host-config/kamori-apply-host-config")
	if err != nil {
		return "", err
	}
	configDispatch, err := deploymentAsset("host-config/kamori-config-dispatch")
	if err != nil {
		return "", err
	}
	repairEgress, err := deploymentAsset("host-config/kamori-repair-egress")
	if err != nil {
		return "", err
	}
	commonFiles := []cloudInitFile{
		{path: "/etc/kamori/node-role", owner: "root:root", permissions: "0644", content: role + "\n"},
		// Cloud-init runs its cc_ssh module after write_files and may replace files
		// under /etc/ssh. Stage the Pulumi-managed identity outside that directory;
		// runcmd installs it after every config module has completed.
		{path: "/var/lib/kamori/bootstrap/ssh_host_ed25519_key", owner: "root:root", permissions: "0600", content: common.hostPrivateKey},
		{path: "/var/lib/kamori/bootstrap/ssh_host_ed25519_key.pub", owner: "root:root", permissions: "0644", content: common.hostPublicKey},
		{path: "/etc/kamori/config-authorized-key", owner: "root:root", permissions: "0600", content: configAuthorizedKey(role, common.configPublicKey)},
		{path: "/usr/local/sbin/kamori-apply-host-config", owner: "root:root", permissions: "0755", content: applyHostConfig},
		{path: "/usr/local/sbin/kamori-config-dispatch", owner: "root:root", permissions: "0755", content: configDispatch},
		{path: "/usr/local/sbin/kamori-repair-egress", owner: "root:root", permissions: "0755", content: repairEgress},
		{path: "/etc/sudoers.d/kamori-configure", owner: "root:root", permissions: "0440", content: configurationSudoers(role)},
		{path: "/etc/ssh/sshd_config.d/60-kamori-hardening.conf", owner: "root:root", permissions: "0644", content: fmt.Sprintf(`Port %s
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin prohibit-password
X11Forwarding no
HostKey /etc/ssh/ssh_host_ed25519_key
`, sshPort)},
		{path: "/etc/systemd/system/ssh.socket.d/60-kamori-listen.conf", owner: "root:root", permissions: "0644", content: fmt.Sprintf(`[Socket]
ListenStream=
ListenStream=%s
`, sshPort)},
		{path: "/etc/fail2ban/jail.d/kamori-sshd.local", owner: "root:root", permissions: "0644", content: fmt.Sprintf(`[sshd]
enabled = true
port = %s
backend = systemd
`, sshPort)},
		{path: "/etc/sysctl.d/60-kamori-hardening.conf", owner: "root:root", permissions: "0644", content: `kernel.kptr_restrict=2
kernel.dmesg_restrict=1
fs.protected_hardlinks=1
fs.protected_symlinks=1
`},
		{path: "/usr/local/sbin/kamori-first-boot", owner: "root:root", permissions: "0755", content: firstBootScript},
	}
	if role != "ops" {
		commonFiles = append(commonFiles,
			cloudInitFile{path: "/usr/local/sbin/kamori-private-default-route", owner: "root:root", permissions: "0755", content: `#!/usr/bin/env bash
set -euo pipefail
for attempt in $(seq 1 120); do
  private_interface=$(ip -o -4 addr show | awk '$4 ~ /^10\.42\./ {print $2; exit}')
  if [[ -n "$private_interface" ]]; then
    ip route replace default via 10.42.0.1 dev "$private_interface" onlink
    resolvectl dns "$private_interface" 185.12.64.1 185.12.64.2
    resolvectl domain "$private_interface" '~.'
    resolvectl default-route "$private_interface" yes
    exit 0
  fi
  sleep 1
done
echo "private network interface did not become available" >&2
exit 1
`},
			cloudInitFile{path: "/etc/systemd/system/kamori-private-egress.service", owner: "root:root", permissions: "0644", content: `[Unit]
Description=Route private-only host egress through the Kamori NAT gateway
After=network-online.target
Wants=network-online.target
Before=docker.service postgresql.service

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/kamori-private-default-route
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
`},
		)
	}
	files = append(commonFiles, files...)

	var document strings.Builder
	document.WriteString("#cloud-config\nhostname: ")
	document.WriteString(common.hostName)
	document.WriteString("\nmanage_etc_hosts: true\npackage_update: false\npackage_upgrade: false\nwrite_files:\n")
	for _, file := range files {
		encoded, err := compressedBase64(file.content)
		if err != nil {
			return "", fmt.Errorf("compress cloud-init file %s: %w", file.path, err)
		}
		fmt.Fprintf(&document, "  - path: %s\n    owner: %s\n    permissions: '%s'\n    encoding: gzip+base64\n    content: %s\n", file.path, file.owner, file.permissions, encoded)
	}
	document.WriteString("runcmd:\n  - [/usr/local/sbin/kamori-first-boot]\n")
	if document.Len() > cloudInitMaximumBytes {
		return "", fmt.Errorf("%s cloud-init is %d bytes; Hetzner limit is %d", role, document.Len(), cloudInitMaximumBytes)
	}
	return document.String(), nil
}

func commonFirstBoot(packages string, privateEgress bool) string {
	routeSetup := ""
	if privateEgress {
		routeSetup = "/usr/local/sbin/kamori-private-default-route\nsystemctl daemon-reload\nsystemctl enable kamori-private-egress.service\n"
	}
	return fmt.Sprintf(`#!/usr/bin/env bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
%s
cat >/etc/apt/apt.conf.d/99kamori-ipv4 <<'EOF'
Acquire::ForceIPv4 "true";
Acquire::Retries "10";
EOF
	install -d -o root -g root -m 0700 /var/lib/kamori/bootstrap
	install -o root -g root -m 0600 /var/lib/kamori/bootstrap/ssh_host_ed25519_key /etc/ssh/ssh_host_ed25519_key
	install -o root -g root -m 0644 /var/lib/kamori/bootstrap/ssh_host_ed25519_key.pub /etc/ssh/ssh_host_ed25519_key.pub
	rm -f /etc/ssh/ssh_host_rsa_key* /etc/ssh/ssh_host_ecdsa_key*
	if ! id deploy >/dev/null 2>&1; then
	  useradd --create-home --shell /bin/bash deploy
	fi
	passwd --lock deploy >/dev/null
	install -d -o deploy -g deploy -m 0700 /home/deploy/.ssh
	install -o deploy -g deploy -m 0600 /etc/kamori/config-authorized-key /home/deploy/.ssh/authorized_keys
	visudo -cf /etc/sudoers.d/kamori-configure
	install -d -o root -g root -m 0755 /run/sshd
sshd -t
systemctl stop ssh.service
systemctl daemon-reload
systemctl enable ssh.socket
systemctl restart ssh.socket
	rm -f /var/lib/kamori/bootstrap/ssh_host_ed25519_key /var/lib/kamori/bootstrap/ssh_host_ed25519_key.pub
for attempt in $(seq 1 120); do
  if apt-get update && apt-get install -y --no-install-recommends %s; then
    break
  fi
  if [[ "$attempt" == 120 ]]; then
    echo "package installation did not succeed after NAT became available" >&2
    exit 1
  fi
  sleep 5
done
sysctl --system
systemctl enable --now fail2ban.service chrony.service prometheus-node-exporter.service
`, routeSetup, packages)
}

func disableCloudInitNetworkHotplug() string {
	return `
# The private network is attached declaratively when Pulumi creates the VM.
# cloud-init 26.1 otherwise treats Docker veth devices as provider network
# hotplug events, repeatedly queries metadata, and leaves a failed unit.
systemctl disable --now cloud-init-hotplugd.socket
systemctl mask cloud-init-hotplugd.socket cloud-init-hotplugd.service
# A fresh host has no failed state to reset. systemctl exits non-zero when the
# unit is not loaded, which must not fail the entire cloud-init runcmd.
systemctl reset-failed cloud-init-hotplugd.service || true
`
}

func commonHostConfigurationFiles(role string, material commonHostMaterial, releasePublicKey string) ([]cloudInitFile, error) {
	files, err := deploymentFiles(map[string]string{
		"/usr/local/sbin/kamori-apply-host-config": "host-config/kamori-apply-host-config",
		"/usr/local/sbin/kamori-config-dispatch":   "host-config/kamori-config-dispatch",
		"/usr/local/sbin/kamori-repair-egress":     "host-config/kamori-repair-egress",
	})
	if err != nil {
		return nil, err
	}
	files = append(files,
		cloudInitFile{path: "/etc/ssh/ssh_host_ed25519_key-cert.pub", owner: "root:root", permissions: "0644", content: material.hostCertificate},
		cloudInitFile{path: "/etc/ssh/sshd_config.d/61-kamori-host-certificate.conf", owner: "root:root", permissions: "0644", content: "HostCertificate /etc/ssh/ssh_host_ed25519_key-cert.pub\n"},
		cloudInitFile{path: "/home/deploy/.ssh/authorized_keys", owner: "deploy:deploy", permissions: "0600", content: configAuthorizedKey(role, material.configPublicKey) + releaseAuthorizedKey(role, releasePublicKey)},
		cloudInitFile{path: "/etc/sudoers.d/kamori-configure", owner: "root:root", permissions: "0440", content: configurationSudoers(role)},
	)
	if role != "ops" {
		files = append(files, cloudInitFile{
			path:        "/etc/systemd/resolved.conf.d/60-kamori-private-egress.conf",
			owner:       "root:root",
			permissions: "0644",
			content: `[Resolve]
DNS=185.12.64.1 185.12.64.2
`,
		})
	}
	return files, nil
}

func appConfigurationFiles(material appCloudInitMaterial) ([]cloudInitFile, error) {
	files, err := deploymentFiles(map[string]string{
		"/opt/kamori/release/compose.yaml":          "cloud-server/compose.yaml",
		"/usr/local/lib/kamori/deploy-cloud-server": "cloud-server/deploy-cloud-server",
		"/usr/local/sbin/kamori-deploy":             "cloud-server/kamori-deploy",
		"/usr/local/sbin/kamori-deploy-migrate":     "cloud-server/kamori-deploy-migrate",
		"/usr/local/sbin/kamori-registry-login":     "cloud-server/kamori-registry-login",
		"/etc/systemd/system/kamori-cloud.service":  "cloud-server/kamori-cloud.service",
	})
	if err != nil {
		return nil, err
	}
	files = append(files,
		cloudInitFile{path: "/etc/kamori/cloud.env", owner: "root:root", permissions: "0400", content: material.cloudEnvironment},
		cloudInitFile{path: "/etc/kamori/secrets/opaque-server-setup", owner: "root:root", permissions: "0400", content: material.opaqueServerSetup},
		cloudInitFile{path: "/etc/kamori/secrets/refresh-rotation-key", owner: "root:root", permissions: "0400", content: material.refreshRotationKey},
		cloudInitFile{path: "/etc/kamori/postgres-ca.crt", owner: "root:root", permissions: "0444", content: material.postgresCACertificate},
		cloudInitFile{path: "/etc/kamori/postgres-client.crt", owner: "root:root", permissions: "0444", content: material.postgresClientCertificate},
		cloudInitFile{path: "/etc/kamori/postgres-client.key", owner: "root:root", permissions: "0400", content: material.postgresClientPrivateKey},
		cloudInitFile{path: "/etc/sudoers.d/kamori-deploy", owner: "root:root", permissions: "0440", content: `deploy ALL=(root) NOPASSWD: /usr/local/sbin/kamori-deploy *
deploy ALL=(root) NOPASSWD: /usr/local/sbin/kamori-deploy-migrate *
deploy ALL=(root) NOPASSWD: /usr/local/sbin/kamori-registry-login *
`},
	)
	common, err := commonHostConfigurationFiles("app", material.commonHostMaterial, material.deployPublicKey)
	if err != nil {
		return nil, err
	}
	return append(common, files...), nil
}

func renderAppHostConfiguration(material appCloudInitMaterial) (string, error) {
	files, err := appConfigurationFiles(material)
	if err != nil {
		return "", err
	}
	return renderHostConfiguration("app", files)
}

func renderAppCloudInit(material appCloudInitMaterial) (string, error) {
	firstBoot := commonFirstBoot("ca-certificates curl jq unattended-upgrades fail2ban chrony prometheus-node-exporter sudo docker.io docker-compose-v2", true) + disableCloudInitNetworkHotplug() + `
systemctl daemon-reload
systemctl enable --now docker.service
`
	return renderCloudInit("app", material.commonHostMaterial, nil, firstBoot)
}

func opsConfigurationFiles(material opsCloudInitMaterial) ([]cloudInitFile, error) {
	files, err := deploymentFiles(map[string]string{
		"/opt/kamori/ops/compose.yaml":                        "ops/compose.yaml",
		"/opt/kamori/ops/prometheus.yml":                      "ops/prometheus.yml",
		"/opt/kamori/ops/alerts.yml":                          "ops/alerts.yml",
		"/opt/kamori/ops/alertmanager.yml":                    "ops/alertmanager.yml",
		"/opt/kamori/ops/grafana-datasource.yml":              "ops/grafana-datasource.yml",
		"/usr/local/lib/kamori/replicate-blobs":               "backup/replicate-blobs",
		"/etc/systemd/system/kamori-blob-replication.service": "backup/kamori-blob-replication.service",
		"/etc/systemd/system/kamori-blob-replication.timer":   "backup/kamori-blob-replication.timer",
	})
	if err != nil {
		return nil, err
	}
	opsEnvironment := envLine("VALKEY_PASSWORD", material.valkeyPassword) + envLine("GRAFANA_ADMIN_PASSWORD", material.grafanaAdminPassword)
	files = append(files,
		cloudInitFile{path: "/etc/kamori/ops.env", owner: "root:root", permissions: "0600", content: opsEnvironment},
		cloudInitFile{path: "/etc/kamori/backup.env", owner: "root:root", permissions: "0600", content: material.backupEnvironment},
		cloudInitFile{path: "/etc/kamori/secrets/metrics_token", owner: "root:root", permissions: "0400", content: material.metricsBearerToken},
		cloudInitFile{path: "/etc/kamori/tls/postgres-ca.crt", owner: "root:root", permissions: "0644", content: material.postgresCACertificate},
		cloudInitFile{path: "/etc/kamori/tls/jobs-client.crt", owner: "root:root", permissions: "0644", content: material.postgresJobsCertificate},
		cloudInitFile{path: "/etc/kamori/tls/jobs-client.key", owner: "root:root", permissions: "0600", content: material.postgresJobsPrivateKey},
		cloudInitFile{path: "/etc/sysctl.d/70-kamori-nat.conf", owner: "root:root", permissions: "0644", content: "net.ipv4.ip_forward=1\n"},
		cloudInitFile{path: "/usr/local/sbin/kamori-configure-nat", owner: "root:root", permissions: "0755", content: `#!/usr/bin/env bash
set -euo pipefail
wan_interface=$(ip -4 route show default | awk '{print $5; exit}')
test -n "$wan_interface"
sysctl -w net.ipv4.ip_forward=1
iptables -t nat -C POSTROUTING -s 10.42.0.0/16 -o "$wan_interface" -j MASQUERADE 2>/dev/null || \
  iptables -t nat -A POSTROUTING -s 10.42.0.0/16 -o "$wan_interface" -j MASQUERADE
iptables -C DOCKER-USER -s 10.42.0.0/16 -o "$wan_interface" -j ACCEPT 2>/dev/null || \
  iptables -I DOCKER-USER 1 -s 10.42.0.0/16 -o "$wan_interface" -j ACCEPT
iptables -C DOCKER-USER -d 10.42.0.0/16 -i "$wan_interface" -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT 2>/dev/null || \
  iptables -I DOCKER-USER 1 -d 10.42.0.0/16 -i "$wan_interface" -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
`},
		cloudInitFile{path: "/etc/systemd/system/kamori-nat-gateway.service", owner: "root:root", permissions: "0644", content: `[Unit]
Description=Kamori private-network NAT gateway
After=network-online.target docker.service
Wants=network-online.target
Requires=docker.service

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/kamori-configure-nat
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
`},
	)
	common, err := commonHostConfigurationFiles("ops", material.commonHostMaterial, material.deployPublicKey)
	if err != nil {
		return nil, err
	}
	return append(common, files...), nil
}

func renderOpsHostConfiguration(material opsCloudInitMaterial) (string, error) {
	files, err := opsConfigurationFiles(material)
	if err != nil {
		return "", err
	}
	return renderHostConfiguration("ops", files)
}

func renderOpsCloudInit(material opsCloudInitMaterial) (string, error) {
	firstBoot := commonFirstBoot("ca-certificates curl jq unattended-upgrades fail2ban chrony prometheus-node-exporter sudo iptables docker.io docker-compose-v2 postgresql-client rclone", false) + disableCloudInitNetworkHotplug() + `
systemctl enable --now docker.service
`
	return renderCloudInit("ops", material.commonHostMaterial, nil, firstBoot)
}

func databaseConfigurationFiles(material databaseCloudInitMaterial) ([]cloudInitFile, error) {
	files, err := deploymentFiles(map[string]string{
		"/usr/local/lib/kamori/postgres-lib":                     "postgres/postgres-lib",
		"/usr/local/lib/kamori/bootstrap-primary":                "postgres/bootstrap-primary",
		"/usr/local/lib/kamori/run-pgbackrest-backup":            "postgres/run-pgbackrest-backup",
		"/usr/local/lib/kamori/kamori-pgbackrest-backup.service": "postgres/kamori-pgbackrest-backup.service",
		"/usr/local/lib/kamori/kamori-pgbackrest-backup.timer":   "postgres/kamori-pgbackrest-backup.timer",
	})
	if err != nil {
		return nil, err
	}
	files = append(files,
		cloudInitFile{path: "/etc/kamori/postgres.env", owner: "root:root", permissions: "0600", content: material.postgresEnvironment},
		cloudInitFile{path: "/etc/kamori/tls/postgres-ca.crt", owner: "root:root", permissions: "0644", content: material.postgresCACertificate},
		cloudInitFile{path: "/etc/kamori/tls/postgres.crt", owner: "root:root", permissions: "0644", content: material.postgresServerCertificate},
		cloudInitFile{path: "/etc/kamori/tls/postgres.key", owner: "root:root", permissions: "0600", content: material.postgresServerPrivateKey},
	)
	common, err := commonHostConfigurationFiles("db-primary", material.commonHostMaterial, "")
	if err != nil {
		return nil, err
	}
	return append(common, files...), nil
}

func renderDatabaseHostConfiguration(material databaseCloudInitMaterial) (string, error) {
	files, err := databaseConfigurationFiles(material)
	if err != nil {
		return "", err
	}
	return renderHostConfiguration("db-primary", files)
}

func renderDatabaseCloudInit(material databaseCloudInitMaterial) (string, error) {
	firstBoot := commonFirstBoot("ca-certificates curl jq unattended-upgrades fail2ban chrony prometheus-node-exporter postgresql postgresql-client pgbackrest rsync", true) + fmt.Sprintf(`
device=/dev/disk/by-id/scsi-0HC_Volume_%s
for attempt in $(seq 1 120); do
  [[ -b "$device" ]] && break
  if [[ "$attempt" == 120 ]]; then
    echo "PostgreSQL data volume did not appear" >&2
    exit 1
  fi
  sleep 5
done
install -d -m 0755 /srv/kamori-postgres
if ! grep -Fq "$device /srv/kamori-postgres " /etc/fstab; then
  printf '%%s /srv/kamori-postgres ext4 defaults,nofail 0 2\n' "$device" >> /etc/fstab
fi
mountpoint -q /srv/kamori-postgres || mount /srv/kamori-postgres
`, material.volumeID)
	return renderCloudInit("db-primary", material.commonHostMaterial, nil, firstBoot)
}

func renderPostgresEnvironment(appPassword, jobsPassword, backupKeyID, backupApplicationKey, cipherPass string) string {
	return "POSTGRES_VERSION=16\n" +
		"POSTGRES_PRIMARY_IP=" + databasePrimaryPrivateIP + "\n" +
		"POSTGRES_DATA_DIR=/srv/kamori-postgres/postgresql/16/main\n" +
		envLine("POSTGRES_APP_PASSWORD", appPassword) +
		envLine("POSTGRES_JOBS_PASSWORD", jobsPassword) +
		"PGBACKREST_S3_ENDPOINT=" + backblazeEndpoint + "\n" +
		"PGBACKREST_S3_REGION=" + backblazeRegion + "\n" +
		"PGBACKREST_S3_BUCKET=" + backblazePostgresBackupBucket + "\n" +
		envLine("PGBACKREST_S3_KEY_ID", backupKeyID) +
		envLine("PGBACKREST_S3_APPLICATION_KEY", backupApplicationKey) +
		envLine("PGBACKREST_CIPHER_PASS", cipherPass)
}

func renderBackupEnvironment(replicationKeyID, replicationApplicationKey, drAccessKey, drSecretKey, jobsPassword, drBucketName string) string {
	jobsURL := "postgres://" + url.UserPassword("kamori_jobs", jobsPassword).String() + "@" + databasePrimaryPrivateIP + ":" + databasePort + "/" + databaseName +
		"?sslmode=verify-full&sslrootcert=/etc/kamori/tls/postgres-ca.crt&sslcert=/etc/kamori/tls/jobs-client.crt&sslkey=/etc/kamori/tls/jobs-client.key"
	return "PRIMARY_S3_ENDPOINT=" + backblazeEndpoint + "\n" +
		"PRIMARY_S3_REGION=" + backblazeRegion + "\n" +
		"PRIMARY_S3_BUCKET=" + backblazePrimaryBucket + "\n" +
		envLine("PRIMARY_S3_KEY_ID", replicationKeyID) +
		envLine("PRIMARY_S3_APPLICATION_KEY", replicationApplicationKey) +
		"DR_S3_ENDPOINT=" + hetznerObjectEndpoint + "\n" +
		"DR_S3_REGION=" + hetznerObjectLocation + "\n" +
		"DR_S3_BUCKET=" + drBucketName + "\n" +
		envLine("DR_S3_ACCESS_KEY", drAccessKey) +
		envLine("DR_S3_SECRET_KEY", drSecretKey) +
		envLine("JOBS_DATABASE_URL", jobsURL) +
		"BLOB_VERIFY_SAMPLE_COUNT=20\n"
}
