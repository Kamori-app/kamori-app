package main

import (
	"bytes"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func writeFixture(t *testing.T, path, contents string, mode os.FileMode) {
	t.Helper()
	if err := os.WriteFile(path, []byte(contents), mode); err != nil {
		t.Fatal(err)
	}
}

// These tests catch evaluating or reinterpreting secrets at either consumer boundary.
func TestSourcedEnvironmentsPreserveLiteralSecrets(t *testing.T) {
	dir := t.TempDir()
	marker := filepath.Join(dir, "executed")
	value := "synthetic-$KAMORI_AUDIT_SYNTHETIC-'\"-`touch " + marker + "`-$(touch " + marker + ")-\\n-\\'\tactual\nnewline\\\n"
	for _, tc := range []struct {
		name, contents string
		keys           []string
	}{
		{"postgres", renderPostgresEnvironment(value, value, value, value, value), []string{"POSTGRES_APP_PASSWORD", "POSTGRES_JOBS_PASSWORD", "PGBACKREST_S3_KEY_ID", "PGBACKREST_S3_APPLICATION_KEY", "PGBACKREST_CIPHER_PASS"}},
		{"backup", renderBackupEnvironment(value, value, value, value, "jobs", "synthetic-dr"), []string{"PRIMARY_S3_KEY_ID", "PRIMARY_S3_APPLICATION_KEY", "DR_S3_ACCESS_KEY", "DR_S3_SECRET_KEY"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			envPath := filepath.Join(dir, tc.name+".env")
			writeFixture(t, envPath, tc.contents, 0600)
			args := []string{"-eu", "-c", `source "$1"; shift; for key do printf '%s\0' "${!key}"; done`, "roundtrip", envPath}
			cmd := exec.Command("bash", append(args, tc.keys...)...)
			cmd.Env = append(os.Environ(), "KAMORI_AUDIT_SYNTHETIC=expanded")
			actual, err := cmd.Output()
			if err != nil {
				t.Fatal(err)
			}
			values := strings.Split(strings.TrimSuffix(string(actual), "\x00"), "\x00")
			if len(values) != len(tc.keys) {
				t.Fatalf("got %d values, want %d", len(values), len(tc.keys))
			}
			for i, got := range values {
				if got != value {
					t.Errorf("%s did not round trip: got %q want %q", tc.keys[i], got, value)
				}
			}
			if _, err := os.Stat(marker); !os.IsNotExist(err) {
				t.Error("synthetic command substitution executed")
			}
		})
	}
}

func TestComposeEnvironmentsPreserveLiteralSecrets(t *testing.T) {
	docker, err := exec.LookPath("docker")
	if err != nil {
		t.Skip("Docker Compose CLI is not installed")
	}
	dir := t.TempDir()
	marker := filepath.Join(dir, "executed")
	value := "synthetic-$KAMORI_AUDIT_SYNTHETIC-${KAMORI_AUDIT_SYNTHETIC}-$$-'\"-`touch " + marker + "`-$(touch " + marker + ")-\\n-\\'\tactual\nnewline\rbackslash\\"
	cloud := renderCloudEnv(cloudEnvSecrets{jwtSecret: value, adminTotpKek: value, authTotpKek: value, objectStoreKeyID: value, objectStoreSecretKey: value, metricsBearerToken: value}, "https://synthetic.test", "synthetic", "synthetic")
	writeFixture(t, filepath.Join(dir, "values.env"), cloud+composeEnvLine("VALKEY_PASSWORD", value)+composeEnvLine("GRAFANA_ADMIN_PASSWORD", value), 0600)
	writeFixture(t, filepath.Join(dir, "compose.yaml"), `services:
  file:
    image: scratch
    env_file: values.env
  substitution:
    image: scratch
    environment:
      VALKEY_PASSWORD: ${VALKEY_PASSWORD}
      GRAFANA_ADMIN_PASSWORD: ${GRAFANA_ADMIN_PASSWORD}
`, 0600)
	cmd := exec.Command(docker, "compose", "--project-directory", dir, "--env-file", filepath.Join(dir, "values.env"), "config", "--format", "json")
	cmd.Env = append(os.Environ(), "KAMORI_AUDIT_SYNTHETIC=expanded")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	output, err := cmd.Output()
	if err != nil {
		t.Fatalf("Compose parser: %v: %s", err, stderr.String())
	}
	var config struct {
		Services map[string]struct {
			Environment map[string]string `json:"environment"`
		} `json:"services"`
	}
	if err := json.Unmarshal(output, &config); err != nil {
		t.Fatal(err)
	}
	// Compose's canonical config escapes literal dollars as $$ for reuse as YAML/JSON.
	want := strings.ReplaceAll(value, "$", "$$")
	for service, keys := range map[string][]string{"file": {"KAMORI_JWT_SECRET", "KAMORI_ADMIN_TOTP_KEK", "KAMORI_AUTH_TOTP_KEK", "KAMORI_OBJECT_STORE_ACCESS_KEY_ID", "KAMORI_OBJECT_STORE_SECRET_ACCESS_KEY", "KAMORI_METRICS_BEARER_TOKEN"}, "substitution": {"VALKEY_PASSWORD", "GRAFANA_ADMIN_PASSWORD"}} {
		for _, key := range keys {
			if got := config.Services[service].Environment[key]; got != want {
				t.Errorf("%s/%s: got %q want %q", service, key, got, want)
			}
		}
	}
	if _, err := os.Stat(marker); !os.IsNotExist(err) {
		t.Error("synthetic command substitution executed")
	}
}
