package main

import (
	"strings"
	"testing"
)

func TestRenderCloudEnvQuotesSecretValuesAndKeepsRequiredGuards(t *testing.T) {
	env := renderCloudEnv(cloudEnvSecrets{
		databasePassword:     "p@ss:/?#[]% word",
		valkeyPassword:       "cache@:/?#[]% word",
		jwtSecret:            "jwt\nvalue",
		adminTotpKek:         "admin-key",
		authTotpKek:          "auth-key",
		objectStoreKeyID:     "runtime-id",
		objectStoreSecretKey: "runtime secret",
		metricsBearerToken:   "metrics-token",
	}, "https://s3.example.test", "eu-test-1", "ciphertext")

	required := []string{
		`KAMORI_DATABASE_URL="postgres://kamori_app:p%40ss%3A%2F%3F%23%5B%5D%25%20word@10.42.0.21:5432/kamori?sslmode=verify-full&sslrootcert=/run/secrets/postgres-ca.crt&sslcert=/run/secrets/postgres-client.crt&sslkey=/run/secrets/postgres-client.key"`,
		`KAMORI_VALKEY_URL="redis://:cache%40%3A%2F%3F%23%5B%5D%25%20word@10.42.0.31:6379/0"`,
		`KAMORI_JWT_SECRET="jwt\nvalue"`,
		`KAMORI_OBJECT_STORE_SECRET_ACCESS_KEY="runtime secret"`,
		"KAMORI_OPAQUE_SERVER_SETUP_FILE=/run/secrets/opaque-server-setup",
		"KAMORI_ALLOW_EPHEMERAL_OPAQUE_SETUP=false",
		"KAMORI_REFRESH_ROTATION_KEY_FILE=/run/secrets/refresh-rotation-key",
		"KAMORI_REGISTRATION_ENABLED=false",
	}
	for _, expected := range required {
		if !strings.Contains(env, expected) {
			t.Fatalf("rendered environment is missing %q", expected)
		}
	}
}

func TestBaseCloudInitKeepsSecretsInRootOnlyFiles(t *testing.T) {
	cloudInit := baseCloudInit("app", appHostSecrets{
		opaqueServerSetup:         "opaque-value",
		refreshRotationKey:        "rotation-value",
		runtimeEnv:                "KAMORI_JWT_SECRET=secret\n",
		postgresCACertificate:     "CA CERTIFICATE",
		postgresClientCertificate: "CLIENT CERTIFICATE",
		postgresClientKey:         "CLIENT PRIVATE KEY",
	})
	for _, expected := range []string{
		"/etc/kamori/secrets/opaque-server-setup",
		"/etc/kamori/secrets/refresh-rotation-key",
		"/etc/kamori/cloud.env",
		"/etc/kamori/postgres-ca.crt",
		"/etc/kamori/postgres-client.crt",
		"/etc/kamori/postgres-client.key",
		"permissions: '0400'",
		"chown, '10001:10001', /etc/kamori/cloud.env",
		"Port 2022",
		"port = 2022",
		"[sshd, -t]",
	} {
		if !strings.Contains(cloudInit, expected) {
			t.Fatalf("cloud-init is missing %q", expected)
		}
	}
}
