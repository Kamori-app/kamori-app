package main

import (
	"os"
	"regexp"
	"testing"
)

func TestPulumiCLIPinMatchesInfrastructureWorkflow(t *testing.T) {
	t.Parallel()

	project, err := os.ReadFile("Pulumi.yaml")
	if err != nil {
		t.Fatalf("read Pulumi.yaml: %v", err)
	}
	workflow, err := os.ReadFile("../.github/workflows/infrastructure.yml")
	if err != nil {
		t.Fatalf("read infrastructure workflow: %v", err)
	}

	projectVersion := requiredMatch(t, project,
		`(?m)^requiredPulumiVersion: "=([0-9]+\.[0-9]+\.[0-9]+)"$`)
	workflowVersion := requiredMatch(t, workflow,
		`(?m)^\s+pulumi-version: ([0-9]+\.[0-9]+\.[0-9]+)$`)

	if projectVersion != workflowVersion {
		t.Fatalf("Pulumi CLI pins differ: Pulumi.yaml=%s workflow=%s",
			projectVersion, workflowVersion)
	}
}

func TestPulumiBackendMatchesInfrastructureWorkflow(t *testing.T) {
	t.Parallel()

	project, err := os.ReadFile("Pulumi.yaml")
	if err != nil {
		t.Fatalf("read Pulumi.yaml: %v", err)
	}
	workflow, err := os.ReadFile("../.github/workflows/infrastructure.yml")
	if err != nil {
		t.Fatalf("read infrastructure workflow: %v", err)
	}

	projectBackend := requiredMatch(t, project, `(?m)^  url: (s3://\S+)$`)
	workflowBackend := requiredMatch(t, workflow, `(?m)^\s+cloud-url: (s3://\S+)$`)
	if projectBackend != workflowBackend {
		t.Fatalf("Pulumi backend URLs differ: Pulumi.yaml=%s workflow=%s",
			projectBackend, workflowBackend)
	}

	if !regexp.MustCompile(`[?&]awssdk=v2(?:&|$)`).MatchString(projectBackend) {
		t.Fatal("Backblaze B2 backend must use the proven AWS SDK v2 path")
	}
}

func requiredMatch(t *testing.T, contents []byte, pattern string) string {
	t.Helper()

	matches := regexp.MustCompile(pattern).FindSubmatch(contents)
	if len(matches) != 2 {
		t.Fatalf("expected exactly one captured match for %q", pattern)
	}
	return string(matches[1])
}
