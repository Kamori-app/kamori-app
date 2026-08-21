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

func requiredMatch(t *testing.T, contents []byte, pattern string) string {
	t.Helper()

	matches := regexp.MustCompile(pattern).FindSubmatch(contents)
	if len(matches) != 2 {
		t.Fatalf("expected exactly one captured match for %q", pattern)
	}
	return string(matches[1])
}
