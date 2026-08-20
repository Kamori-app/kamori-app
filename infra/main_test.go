package main

import "testing"

func TestLoadBalancerAlgorithmUsesProviderWireValue(t *testing.T) {
	if loadBalancerAlgorithm != "least_connections" {
		t.Fatalf("loadBalancerAlgorithm = %q, want provider wire value %q", loadBalancerAlgorithm, "least_connections")
	}
}

func TestDatabaseStandbyMode(t *testing.T) {
	tests := []struct {
		name       string
		configured string
		want       string
		wantError  bool
	}{
		{name: "defaults to enabled", want: standbyModeEnabled},
		{name: "enabled", configured: standbyModeEnabled, want: standbyModeEnabled},
		{name: "retiring", configured: standbyModeRetiring, want: standbyModeRetiring},
		{name: "disabled", configured: standbyModeDisabled, want: standbyModeDisabled},
		{name: "invalid", configured: "off", wantError: true},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := normalizeDatabaseStandbyMode(test.configured)
			if test.wantError {
				if err == nil {
					t.Fatal("normalizeDatabaseStandbyMode returned no error")
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			if got != test.want {
				t.Fatalf("normalizeDatabaseStandbyMode = %q, want %q", got, test.want)
			}
		})
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
