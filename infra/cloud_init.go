package main

import (
	"bytes"
	"compress/gzip"
	"encoding/base64"
	"fmt"
	"net/url"
	"os"
	"path/filepath"
	"sort"
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
	hostCertificate string
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

func renderCloudInit(role string, common commonHostMaterial, files []cloudInitFile, firstBootScript string) (string, error) {
	commonFiles := []cloudInitFile{
		{path: "/etc/kamori/node-role", owner: "root:root", permissions: "0644", content: role + "\n"},
		{path: "/etc/ssh/ssh_host_ed25519_key", owner: "root:root", permissions: "0600", content: common.hostPrivateKey},
		{path: "/etc/ssh/ssh_host_ed25519_key-cert.pub", owner: "root:root", permissions: "0644", content: common.hostCertificate},
		{path: "/etc/ssh/sshd_config.d/60-kamori-hardening.conf", owner: "root:root", permissions: "0644", content: fmt.Sprintf(`Port %s
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitRootLogin prohibit-password
X11Forwarding no
HostKey /etc/ssh/ssh_host_ed25519_key
HostCertificate /etc/ssh/ssh_host_ed25519_key-cert.pub
HostKeyAlgorithms ssh-ed25519-cert-v01@openssh.com
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
sshd -t
systemctl daemon-reload
systemctl enable ssh.socket
systemctl restart ssh.socket
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
rm -f /etc/ssh/ssh_host_rsa_key* /etc/ssh/ssh_host_ecdsa_key*
sysctl --system
systemctl enable --now fail2ban.service chrony.service prometheus-node-exporter.service
`, routeSetup, packages)
}

func renderAppCloudInit(material appCloudInitMaterial) (string, error) {
	files, err := deploymentFiles(map[string]string{
		"/opt/kamori/release/compose.yaml":          "cloud-server/compose.yaml",
		"/usr/local/lib/kamori/deploy-cloud-server": "cloud-server/deploy-cloud-server",
		"/usr/local/sbin/kamori-deploy":             "cloud-server/kamori-deploy",
		"/usr/local/sbin/kamori-deploy-migrate":     "cloud-server/kamori-deploy-migrate",
		"/usr/local/sbin/kamori-registry-login":     "cloud-server/kamori-registry-login",
		"/etc/systemd/system/kamori-cloud.service":  "cloud-server/kamori-cloud.service",
	})
	if err != nil {
		return "", err
	}
	files = append(files,
		cloudInitFile{path: "/etc/kamori/deploy.pub", owner: "root:root", permissions: "0444", content: material.deployPublicKey},
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
	firstBoot := commonFirstBoot("ca-certificates curl jq unattended-upgrades fail2ban chrony prometheus-node-exporter sudo docker.io docker-compose-v2", true) + `
if ! id deploy >/dev/null 2>&1; then
  useradd --create-home --shell /bin/bash deploy
fi
passwd --lock deploy >/dev/null
install -d -o deploy -g deploy -m 0700 /home/deploy/.ssh
{ printf 'restrict '; cat /etc/kamori/deploy.pub; } > /home/deploy/.ssh/authorized_keys
chown deploy:deploy /home/deploy/.ssh/authorized_keys
chmod 0600 /home/deploy/.ssh/authorized_keys
visudo -cf /etc/sudoers.d/kamori-deploy
chown 10001:10001 \
  /etc/kamori/cloud.env \
  /etc/kamori/secrets/opaque-server-setup \
  /etc/kamori/secrets/refresh-rotation-key \
  /etc/kamori/postgres-ca.crt \
  /etc/kamori/postgres-client.crt \
  /etc/kamori/postgres-client.key
systemctl daemon-reload
systemctl enable --now docker.service
systemctl enable kamori-cloud.service
`
	return renderCloudInit("app", material.commonHostMaterial, files, firstBoot)
}

func renderOpsCloudInit(material opsCloudInitMaterial) (string, error) {
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
		return "", err
	}
	opsEnvironment := envLine("VALKEY_PASSWORD", material.valkeyPassword) + envLine("GRAFANA_ADMIN_PASSWORD", material.grafanaAdminPassword)
	files = append(files,
		cloudInitFile{path: "/etc/kamori/deploy.pub", owner: "root:root", permissions: "0444", content: material.deployPublicKey},
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
	firstBoot := commonFirstBoot("ca-certificates curl jq unattended-upgrades fail2ban chrony prometheus-node-exporter sudo iptables docker.io docker-compose-v2 postgresql-client rclone", false) + `
if ! id deploy >/dev/null 2>&1; then
  useradd --create-home --shell /bin/bash deploy
fi
passwd --lock deploy >/dev/null
install -d -o deploy -g deploy -m 0700 /home/deploy/.ssh
	{
	  printf 'command="/bin/false",restrict,port-forwarding,permitopen="10.42.0.11:2022",permitopen="10.42.0.12:2022" '
  cat /etc/kamori/deploy.pub
} > /home/deploy/.ssh/authorized_keys
chown deploy:deploy /home/deploy/.ssh/authorized_keys
chmod 0600 /home/deploy/.ssh/authorized_keys
systemctl enable --now docker.service
systemctl daemon-reload
systemctl enable --now kamori-nat-gateway.service
cd /opt/kamori/ops
docker compose --env-file /etc/kamori/ops.env config --quiet
docker compose --env-file /etc/kamori/ops.env pull
docker compose --env-file /etc/kamori/ops.env up -d --remove-orphans
systemctl daemon-reload
systemctl enable --now kamori-blob-replication.timer
`
	return renderCloudInit("ops", material.commonHostMaterial, files, firstBoot)
}

func renderDatabaseCloudInit(material databaseCloudInitMaterial) (string, error) {
	files, err := deploymentFiles(map[string]string{
		"/usr/local/lib/kamori/postgres-lib":                     "postgres/postgres-lib",
		"/usr/local/lib/kamori/bootstrap-primary":                "postgres/bootstrap-primary",
		"/usr/local/lib/kamori/run-pgbackrest-backup":            "postgres/run-pgbackrest-backup",
		"/usr/local/lib/kamori/kamori-pgbackrest-backup.service": "postgres/kamori-pgbackrest-backup.service",
		"/usr/local/lib/kamori/kamori-pgbackrest-backup.timer":   "postgres/kamori-pgbackrest-backup.timer",
	})
	if err != nil {
		return "", err
	}
	files = append(files,
		cloudInitFile{path: "/etc/kamori/postgres.env", owner: "root:root", permissions: "0600", content: material.postgresEnvironment},
		cloudInitFile{path: "/etc/kamori/tls/postgres-ca.crt", owner: "root:root", permissions: "0644", content: material.postgresCACertificate},
		cloudInitFile{path: "/etc/kamori/tls/postgres.crt", owner: "root:root", permissions: "0644", content: material.postgresServerCertificate},
		cloudInitFile{path: "/etc/kamori/tls/postgres.key", owner: "root:root", permissions: "0600", content: material.postgresServerPrivateKey},
	)
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
/usr/local/lib/kamori/bootstrap-primary
`, material.volumeID)
	return renderCloudInit("db-primary", material.commonHostMaterial, files, firstBoot)
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
