package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// Execute the deployed jobs, replacing only external tools and absolute host paths.
func TestBackupJobsRecordExactlyOneTerminalHeartbeat(t *testing.T) {
	hashTool, err := exec.LookPath("sha256sum")
	if err != nil {
		t.Fatal("sha256sum is required to exercise actual ciphertext hashing: ", err)
	}
	for _, tc := range []struct {
		job, stage string
		code       int
	}{
		{"postgres", "success", 0}, {"postgres", "backup", 41}, {"postgres", "archive-check", 42},
		{"replication", "success", 0}, {"replication", "copy", 43}, {"replication", "check", 44},
		{"replication", "primary-inventory", 45}, {"replication", "dr-inventory", 46},
		{"replication", "primary-cat", 47}, {"replication", "dr-cat", 48}, {"replication", "hash", 49},
		{"replication", "missing", 1}, {"replication", "mismatch", 1},
		{"replication", "empty-dr", 1}, {"replication", "both-empty", 0},
	} {
		t.Run(tc.job+"/"+tc.stage, func(t *testing.T) {
			dir := t.TempDir()
			bin := filepath.Join(dir, "bin")
			if err := os.Mkdir(bin, 0700); err != nil {
				t.Fatal(err)
			}
			fake := `#!/usr/bin/env bash
set -eu
case "${0##*/}" in
runuser) shift 3; exec "$@" ;;
psql) for arg do case "$arg" in --set=status=*) printf '%s\n' "${arg#--set=status=}" >> "$HEARTBEATS";; esac; done; cat >/dev/null ;;
pgbackrest)
 case "$*" in *' backup') [[ "$FAIL_STAGE" != backup ]] || exit 41;; *' check') [[ "$FAIL_STAGE" != archive-check ]] || exit 42;; *) exit 90;; esac ;;
flock) exit 0 ;;
shuf) head -n "$2" ;;
sha256sum) [[ "$FAIL_STAGE" != hash ]] || exit 49; "$REAL_SHA256SUM" "$@" ;;
rclone)
 case "$1" in
 copy) [[ "$FAIL_STAGE" != copy ]] || exit 43 ;;
 check) [[ "$FAIL_STAGE" != check ]] || exit 44 ;;
 lsf) [[ "$FAIL_STAGE" != both-empty ]] || exit 0
 case "$2" in
  primary:*) [[ "$FAIL_STAGE" != primary-inventory ]] || exit 45 ;;
  dr:*) [[ "$FAIL_STAGE" != dr-inventory ]] || exit 46; [[ "$FAIL_STAGE" != empty-dr ]] || exit 0; if [[ "$FAIL_STAGE" == missing ]]; then printf 'unrelated;7\n'; exit 0; fi ;;
 esac; printf 'object;7\n' ;;
 cat) case "$2" in
  primary:*) [[ "$FAIL_STAGE" != primary-cat ]] || exit 47 ;;
  dr:*) [[ "$FAIL_STAGE" != dr-cat ]] || exit 48; if [[ "$FAIL_STAGE" == mismatch ]]; then printf different; exit 0; fi ;;
 esac; printf payload ;;
 *) exit 91 ;;
 esac ;;
*) exit 92 ;;
esac
`
			for _, name := range []string{"runuser", "psql", "pgbackrest", "flock", "shuf", "sha256sum", "rclone"} {
				writeFixture(t, filepath.Join(bin, name), fake, 0700)
			}
			envPath := filepath.Join(dir, "synthetic.env")
			var asset, env string
			if tc.job == "postgres" {
				asset = "postgres/run-pgbackrest-backup"
				env = renderPostgresEnvironment("synthetic", "synthetic", "synthetic", "synthetic", "synthetic")
				lib, err := deploymentAsset("postgres/postgres-lib")
				if err != nil {
					t.Fatal(err)
				}
				lib = strings.ReplaceAll(lib, "/etc/kamori/postgres.env", envPath)
				writeFixture(t, filepath.Join(dir, "postgres-lib"), lib, 0600)
			} else {
				asset = "backup/replicate-blobs"
				env = renderBackupEnvironment("synthetic", "synthetic", "synthetic", "synthetic", "synthetic", "synthetic-dr")
			}
			writeFixture(t, envPath, env, 0600)
			script, err := deploymentAsset(asset)
			if err != nil {
				t.Fatal(err)
			}
			script = strings.ReplaceAll(script, "/usr/local/lib/kamori/postgres-lib", filepath.Join(dir, "postgres-lib"))
			script = strings.ReplaceAll(script, "/run/lock/kamori-blob-replication.lock", filepath.Join(dir, "lock"))
			scriptPath := filepath.Join(dir, "job")
			writeFixture(t, scriptPath, script, 0700)
			log := filepath.Join(dir, "heartbeats")
			cmd := exec.Command("bash", scriptPath)
			cmd.Env = append(os.Environ(), "PATH="+bin+":"+os.Getenv("PATH"), "KAMORI_BACKUP_ENV="+envPath, "HEARTBEATS="+log, "FAIL_STAGE="+tc.stage, "REAL_SHA256SUM="+hashTool)
			output, err := cmd.CombinedOutput()
			code := 0
			if err != nil {
				if exit, ok := err.(*exec.ExitError); ok {
					code = exit.ExitCode()
				} else {
					t.Fatal(err)
				}
			}
			if code != tc.code {
				t.Errorf("exit status=%d want %d; output: %s", code, tc.code, output)
			}
			heartbeats, err := os.ReadFile(log)
			if err != nil {
				t.Fatal(err)
			}
			want := "running\nok\n"
			if tc.code != 0 {
				want = "running\nfailed\n"
			}
			if string(heartbeats) != want {
				t.Errorf("heartbeats=%q want %q; output: %s", heartbeats, want, output)
			}
		})
	}
}
