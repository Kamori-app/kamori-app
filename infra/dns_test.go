package main

import (
	"slices"
	"testing"
)

func TestPublicDNSDomainsMatchEdgeRoutes(t *testing.T) {
	want := []string{
		"kamori.app",
		"app.kamori.app",
		"api.kamori.app",
		"admin.kamori.app",
	}
	if got := publicDNSDomains(); !slices.Equal(got, want) {
		t.Fatalf("public DNS domains = %v, want %v", got, want)
	}
}

func TestPublicDNSResourceNameUsesStableApexName(t *testing.T) {
	if got := publicDNSResourceName(""); got != "apex" {
		t.Fatalf("apex resource name = %q, want %q", got, "apex")
	}
	if got := publicDNSResourceName("api"); got != "api" {
		t.Fatalf("api resource name = %q, want %q", got, "api")
	}
}

func TestAcmeDelegationSubdomainsCoverEveryCertificateName(t *testing.T) {
	want := []string{
		"_acme-challenge",
		"_acme-challenge.app",
		"_acme-challenge.api",
		"_acme-challenge.admin",
	}
	got := make([]string, 0, len(publicDNSSubdomains))
	for _, subdomain := range publicDNSSubdomains {
		got = append(got, acmeDelegationSubdomain(subdomain))
	}
	if !slices.Equal(got, want) {
		t.Fatalf("ACME delegation subdomains = %v, want %v", got, want)
	}
}
