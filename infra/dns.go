package main

import (
	"fmt"

	"github.com/pulumi/pulumi-hcloud/sdk/go/hcloud"
	porkbun "github.com/pulumi/pulumi-terraform-provider/sdks/go/porkbun/porkbun"
	"github.com/pulumi/pulumi/sdk/v3/go/pulumi"
	"github.com/pulumi/pulumi/sdk/v3/go/pulumi/config"
)

const (
	publicDNSDomain        = "kamori.app"
	acmeChallengeDNSName   = "_acme-challenge"
	publicDNSTTL           = 600
	hetznerNameserverCount = 3
)

var publicDNSSubdomains = []string{"", "app", "api", "admin"}

type publicEdgeResources struct {
	certificate     *hcloud.ManagedCertificate
	certificateZone *hcloud.Zone
}

func publicDNSDomains() []string {
	domains := make([]string, 0, len(publicDNSSubdomains))
	for _, subdomain := range publicDNSSubdomains {
		domains = append(domains, publicDNSFQDN(subdomain))
	}
	return domains
}

func publicDNSFQDN(subdomain string) string {
	if subdomain == "" {
		return publicDNSDomain
	}
	return subdomain + "." + publicDNSDomain
}

func publicDNSResourceName(subdomain string) string {
	if subdomain == "" {
		return "apex"
	}
	return subdomain
}

func acmeDelegationSubdomain(subdomain string) string {
	if subdomain == "" {
		return acmeChallengeDNSName
	}
	return acmeChallengeDNSName + "." + subdomain
}

func provisionPublicDNSAndTLS(
	ctx *pulumi.Context,
	cfg *config.Config,
	hcloudProvider *hcloud.Provider,
	loadBalancer *hcloud.LoadBalancer,
) (*publicEdgeResources, error) {
	hcloudOpts := pulumi.Provider(hcloudProvider)

	certificateZone, err := hcloud.NewZone(ctx, "certificate-dns-zone", &hcloud.ZoneArgs{
		Name:             pulumi.String(publicDNSDomain),
		Mode:             pulumi.String("primary"),
		Ttl:              pulumi.Int(publicDNSTTL),
		DeleteProtection: pulumi.Bool(true),
		Labels:           commonLabels("acme-dns"),
	}, hcloudOpts, pulumi.Protect(true))
	if err != nil {
		return nil, fmt.Errorf("create Hetzner ACME DNS zone: %w", err)
	}

	porkbunProvider, err := porkbun.NewProvider(ctx, "porkbun", &porkbun.ProviderArgs{
		ApiKey:       cfg.RequireSecret("porkbunApiKey").ToStringPtrOutput(),
		SecretApiKey: cfg.RequireSecret("porkbunSecretApiKey").ToStringPtrOutput(),
		MaxRetries:   pulumi.Float64(5).ToFloat64PtrOutput(),
	})
	if err != nil {
		return nil, fmt.Errorf("create Porkbun provider: %w", err)
	}
	porkbunOpts := pulumi.Provider(porkbunProvider)

	assignedNameservers := certificateZone.AuthoritativeNameservers.Assigneds()
	acmeDelegations := make([]pulumi.Resource, 0, len(publicDNSSubdomains)*hetznerNameserverCount)
	for _, subdomain := range publicDNSSubdomains {
		delegationName := acmeDelegationSubdomain(subdomain)
		resourceName := publicDNSResourceName(subdomain)
		for index := 0; index < hetznerNameserverCount; index++ {
			delegation, err := porkbun.NewDnsRecord(ctx, fmt.Sprintf("acme-delegation-%s-%d", resourceName, index+1), &porkbun.DnsRecordArgs{
				Domain:    pulumi.String(publicDNSDomain),
				Subdomain: pulumi.String(delegationName),
				Type:      pulumi.String("NS"),
				Content:   assignedNameservers.Index(pulumi.Int(index)),
				Ttl:       pulumi.Float64(publicDNSTTL).ToFloat64PtrOutput(),
			}, porkbunOpts, pulumi.Protect(true))
			if err != nil {
				return nil, fmt.Errorf("create Porkbun ACME delegation %d for %s: %w", index+1, publicDNSFQDN(delegationName), err)
			}
			acmeDelegations = append(acmeDelegations, delegation)
		}
	}

	for _, subdomain := range publicDNSSubdomains {
		resourceName := publicDNSResourceName(subdomain)
		fqdn := publicDNSFQDN(subdomain)
		for _, record := range []struct {
			name     string
			typeName string
			content  pulumi.StringInput
		}{
			{name: "ipv4", typeName: "A", content: loadBalancer.Ipv4},
			{name: "ipv6", typeName: "AAAA", content: loadBalancer.Ipv6},
		} {
			_, err := porkbun.NewDnsRecord(ctx, "public-"+resourceName+"-"+record.name, &porkbun.DnsRecordArgs{
				Domain:    pulumi.String(publicDNSDomain),
				Subdomain: pulumi.String(subdomain),
				Type:      pulumi.String(record.typeName),
				Content:   record.content,
				Ttl:       pulumi.Float64(publicDNSTTL).ToFloat64PtrOutput(),
			}, porkbunOpts, pulumi.Protect(true))
			if err != nil {
				return nil, fmt.Errorf("create Porkbun %s record for %s: %w", record.typeName, fqdn, err)
			}
		}
	}

	certificate, err := hcloud.NewManagedCertificate(ctx, "public-tls-certificate", &hcloud.ManagedCertificateArgs{
		Name:        pulumi.String("kamori-beta-public"),
		DomainNames: stringsToInputs(publicDNSDomains()),
		Labels:      commonLabels("public-tls"),
	}, hcloudOpts, pulumi.DependsOn(acmeDelegations), pulumi.Protect(true))
	if err != nil {
		return nil, fmt.Errorf("create Hetzner managed certificate: %w", err)
	}

	return &publicEdgeResources{
		certificate:     certificate,
		certificateZone: certificateZone,
	}, nil
}
