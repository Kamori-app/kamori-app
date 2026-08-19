package main

import "testing"

func TestLoadBalancerAlgorithmUsesProviderWireValue(t *testing.T) {
	if loadBalancerAlgorithm != "least_connections" {
		t.Fatalf("loadBalancerAlgorithm = %q, want provider wire value %q", loadBalancerAlgorithm, "least_connections")
	}
}
