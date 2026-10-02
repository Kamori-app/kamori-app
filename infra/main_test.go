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

func TestDeploymentModeIsExplicitlyBounded(t *testing.T) {
	for _, mode := range []string{deploymentModeActive, deploymentModePaused} {
		if err := validateDeploymentMode(mode); err != nil {
			t.Fatalf("valid deployment mode %q rejected: %v", mode, err)
		}
	}
	if err := validateDeploymentMode("destroyed"); err == nil {
		t.Fatal("unknown deployment mode was accepted")
	}
}

func TestPausedDeploymentRemovesRuntimeAndProtectsDatabaseVolume(t *testing.T) {
	lifecycle := deploymentLifecycleFor(deploymentModePaused, hostProvisioningRetire)
	if lifecycle.active {
		t.Fatal("paused deployment unexpectedly provisions runtime resources")
	}
	if lifecycle.runtimeProtected {
		t.Fatal("absent paused runtime unexpectedly has protection enabled")
	}
	if !lifecycle.volumeProtected {
		t.Fatal("paused deployment does not protect the retained database volume")
	}
}

func TestRetirePhaseUnprotectsActiveRuntimeBeforePause(t *testing.T) {
	lifecycle := deploymentLifecycleFor(deploymentModeActive, hostProvisioningRetire)
	if !lifecycle.active {
		t.Fatal("retire phase removed runtime before deletion protection was disabled")
	}
	if lifecycle.runtimeProtected || lifecycle.volumeProtected {
		t.Fatal("retire phase did not disable active runtime and volume protection")
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

func TestDatabaseVolumeSizeCannotDropBelowRecoveryFloor(t *testing.T) {
	for _, size := range []int{minimumDatabaseVolumeGB, 160, 1024} {
		if err := validateDatabaseVolumeSizeGB(size); err != nil {
			t.Fatalf("valid database volume size %d rejected: %v", size, err)
		}
	}
	for _, size := range []int{0, 80, minimumDatabaseVolumeGB - 1} {
		if err := validateDatabaseVolumeSizeGB(size); err == nil {
			t.Fatalf("unsafe database volume size %d accepted", size)
		}
	}
}
