package main

import "testing"

func TestLoadBalancerAlgorithmUsesProviderWireValue(t *testing.T) {
	if loadBalancerAlgorithm != "least_connections" {
		t.Fatalf("loadBalancerAlgorithm = %q, want provider wire value %q", loadBalancerAlgorithm, "least_connections")
	}
}

func TestDefaultServerTypesMatchTheProductionArchitecture(t *testing.T) {
	tests := map[string]string{
		"app":      defaultAppServerType,
		"ops":      defaultOpsServerType,
		"database": defaultDatabaseServerType,
	}
	wants := map[string]string{
		"app":      "cx23",
		"ops":      "cx23",
		"database": "cx33",
	}
	for role, got := range tests {
		if got != wants[role] {
			t.Errorf("default %s server type = %q, want %q", role, got, wants[role])
		}
	}
}
