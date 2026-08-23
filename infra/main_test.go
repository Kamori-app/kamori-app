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

func TestHostProvisioningPhaseIsExplicitlyBounded(t *testing.T) {
	for _, phase := range []string{hostProvisioningRetire, hostProvisioningReplace, hostProvisioningProtect} {
		if err := validateHostProvisioningPhase(phase); err != nil {
			t.Fatalf("valid phase %q rejected: %v", phase, err)
		}
	}
	if err := validateHostProvisioningPhase("destroy"); err == nil {
		t.Fatal("unknown destructive phase was accepted")
	}
}

func TestOnlyReplacePhaseAdoptsChangedImmutableUserData(t *testing.T) {
	tests := map[string]hostLifecycle{
		hostProvisioningRetire:  {protected: false, replaceUserData: false},
		hostProvisioningReplace: {protected: false, replaceUserData: true},
		hostProvisioningProtect: {protected: true, replaceUserData: false},
	}
	for phase, want := range tests {
		if got := hostLifecycleForPhase(phase); got != want {
			t.Errorf("host lifecycle for %s = %+v, want %+v", phase, got, want)
		}
	}
}
