package main

import (
	"bytes"
	"crypto/ed25519"
	"crypto/rand"
	"encoding/pem"
	"testing"
	"time"

	"golang.org/x/crypto/ssh"
)

func TestTLSAllowedUsesMatchProviderWireValues(t *testing.T) {
	values := []string{
		tlsUseCertSigning,
		tlsUseCRLSigning,
		tlsUseDigitalSignature,
		tlsUseServerAuth,
		tlsUseClientAuth,
	}
	for _, value := range values {
		for _, character := range value {
			if character >= 'A' && character <= 'Z' {
				t.Fatalf("TLS allowed use %q is not a provider wire value", value)
			}
		}
	}
}

func TestSignSSHHostCertificatePinsTheExpectedPrincipal(t *testing.T) {
	_, caPrivate, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	caBlock, err := ssh.MarshalPrivateKey(caPrivate, "test-ca")
	if err != nil {
		t.Fatal(err)
	}
	hostPublic, _, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		t.Fatal(err)
	}
	hostKey, err := ssh.NewPublicKey(hostPublic)
	if err != nil {
		t.Fatal(err)
	}

	issuedAt := time.Now().UTC().Truncate(time.Hour)
	encoded, err := signSSHHostCertificate(
		string(pem.EncodeToMemory(caBlock)),
		string(ssh.MarshalAuthorizedKey(hostKey)),
		"kamori-beta-app-1",
		1,
		issuedAt,
	)
	if err != nil {
		t.Fatal(err)
	}
	parsed, _, _, _, err := ssh.ParseAuthorizedKey([]byte(encoded))
	if err != nil {
		t.Fatal(err)
	}
	certificate, ok := parsed.(*ssh.Certificate)
	if !ok {
		t.Fatalf("signed key has type %T, want *ssh.Certificate", parsed)
	}
	if certificate.CertType != ssh.HostCert || len(certificate.ValidPrincipals) != 1 || certificate.ValidPrincipals[0] != "kamori-beta-app-1" {
		t.Fatalf("unexpected host certificate identity: %#v", certificate)
	}
	caPublic, err := ssh.NewPublicKey(caPrivate.Public())
	if err != nil {
		t.Fatal(err)
	}
	checker := ssh.CertChecker{IsHostAuthority: func(candidate ssh.PublicKey, _ string) bool {
		return bytes.Equal(candidate.Marshal(), caPublic.Marshal())
	}}
	if err := checker.CheckHostKey("kamori-beta-app-1:2022", nil, certificate); err != nil {
		t.Fatalf("host CA did not authenticate the expected principal: %v", err)
	}

	repeated, err := signSSHHostCertificate(
		string(pem.EncodeToMemory(caBlock)),
		string(ssh.MarshalAuthorizedKey(hostKey)),
		"kamori-beta-app-1",
		1,
		issuedAt,
	)
	if err != nil {
		t.Fatal(err)
	}
	if repeated != encoded {
		t.Fatal("identical host identity produced a different certificate")
	}
	if certificate.ValidBefore == ssh.CertTimeInfinity || certificate.ValidBefore <= certificate.ValidAfter {
		t.Fatalf("host certificate must have a finite validity window: %#v", certificate)
	}
}
