package main

import (
	"strings"
	"testing"

	"gopkg.in/yaml.v3"
)

func TestAutomatedCloudInitIsDeterministicAndBelowProviderLimit(t *testing.T) {
	common := commonHostMaterial{
		hostName:        "kamori-beta-app-1",
		hostPrivateKey:  "HOST PRIVATE KEY",
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
	common := commonHostMaterial{hostName: "kamori-beta-test", hostPrivateKey: "HOST KEY", hostCertificate: "HOST CERT"}
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
