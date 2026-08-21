package main

import (
	"bytes"
	"crypto/sha256"
	"fmt"

	"github.com/pulumi/pulumi-random/sdk/v4/go/random"
	"github.com/pulumi/pulumi-tls/sdk/v5/go/tls"
	"github.com/pulumi/pulumi/sdk/v3/go/pulumi"
	"golang.org/x/crypto/ssh"
)

const (
	postgresCAValidityHours   = 10 * 365 * 24
	postgresLeafValidityHours = 397 * 24
	postgresLeafRenewalHours  = 30 * 24
)

type generatedPasswords struct {
	postgresJobs pulumi.StringOutput
	pgBackRest   pulumi.StringOutput
	grafanaAdmin pulumi.StringOutput
}

type postgresPKI struct {
	caCertificate         pulumi.StringOutput
	serverCertificate     pulumi.StringOutput
	serverPrivateKey      pulumi.StringOutput
	appClientCertificate  pulumi.StringOutput
	appClientPrivateKey   pulumi.StringOutput
	jobsClientCertificate pulumi.StringOutput
	jobsClientPrivateKey  pulumi.StringOutput
}

type sshHostIdentity struct {
	privateKey  pulumi.StringOutput
	certificate pulumi.StringOutput
}

type sshPKI struct {
	caPublicKey      pulumi.StringOutput
	deployPublicKey  pulumi.StringOutput
	deployPrivateKey pulumi.StringOutput
	hosts            map[string]sshHostIdentity
}

func provisionGeneratedPasswords(ctx *pulumi.Context) (*generatedPasswords, error) {
	newPassword := func(name string, length int) (*random.RandomPassword, error) {
		return random.NewRandomPassword(ctx, name, &random.RandomPasswordArgs{
			Length:  pulumi.Int(length),
			Lower:   pulumi.Bool(true),
			Numeric: pulumi.Bool(true),
			Special: pulumi.Bool(false),
			Upper:   pulumi.Bool(true),
		})
	}

	postgresJobs, err := newPassword("postgres-jobs-password", 48)
	if err != nil {
		return nil, fmt.Errorf("create PostgreSQL jobs password: %w", err)
	}
	pgBackRest, err := newPassword("pgbackrest-cipher-pass", 64)
	if err != nil {
		return nil, fmt.Errorf("create pgBackRest cipher passphrase: %w", err)
	}
	grafanaAdmin, err := newPassword("grafana-admin-password", 48)
	if err != nil {
		return nil, fmt.Errorf("create Grafana admin password: %w", err)
	}

	return &generatedPasswords{
		postgresJobs: postgresJobs.Result,
		pgBackRest:   pgBackRest.Result,
		grafanaAdmin: grafanaAdmin.Result,
	}, nil
}

func provisionPostgresPKI(ctx *pulumi.Context) (*postgresPKI, error) {
	caKey, err := tls.NewPrivateKey(ctx, "postgres-ca-key", &tls.PrivateKeyArgs{
		Algorithm:  pulumi.String("ECDSA"),
		EcdsaCurve: pulumi.String("P384"),
	})
	if err != nil {
		return nil, fmt.Errorf("create PostgreSQL CA key: %w", err)
	}
	caCert, err := tls.NewSelfSignedCert(ctx, "postgres-ca-certificate", &tls.SelfSignedCertArgs{
		PrivateKeyPem:       caKey.PrivateKeyPem,
		IsCaCertificate:     pulumi.Bool(true),
		MaxPathLength:       pulumi.Int(0),
		SetAuthorityKeyId:   pulumi.Bool(true),
		SetSubjectKeyId:     pulumi.Bool(true),
		ValidityPeriodHours: pulumi.Int(postgresCAValidityHours),
		AllowedUses: pulumi.StringArray{
			pulumi.String("certSigning"),
			pulumi.String("crlSigning"),
		},
		Subject: &tls.SelfSignedCertSubjectArgs{
			CommonName:   pulumi.String("Kamori PostgreSQL Root CA"),
			Organization: pulumi.String("Kamori"),
		},
	})
	if err != nil {
		return nil, fmt.Errorf("create PostgreSQL CA certificate: %w", err)
	}

	type leaf struct {
		certificate pulumi.StringOutput
		privateKey  pulumi.StringOutput
	}
	issueLeaf := func(resourceName, commonName, allowedUse string, ipAddresses, uris pulumi.StringArray) (*leaf, error) {
		key, err := tls.NewPrivateKey(ctx, resourceName+"-key", &tls.PrivateKeyArgs{
			Algorithm:  pulumi.String("ECDSA"),
			EcdsaCurve: pulumi.String("P384"),
		})
		if err != nil {
			return nil, err
		}
		request, err := tls.NewCertRequest(ctx, resourceName+"-request", &tls.CertRequestArgs{
			PrivateKeyPem: key.PrivateKeyPem,
			IpAddresses:   ipAddresses,
			Uris:          uris,
			Subject: &tls.CertRequestSubjectArgs{
				CommonName:   pulumi.String(commonName),
				Organization: pulumi.String("Kamori"),
			},
		})
		if err != nil {
			return nil, err
		}
		certificate, err := tls.NewLocallySignedCert(ctx, resourceName+"-certificate", &tls.LocallySignedCertArgs{
			CertRequestPem:      request.CertRequestPem,
			CaCertPem:           caCert.CertPem,
			CaPrivateKeyPem:     caKey.PrivateKeyPem,
			ValidityPeriodHours: pulumi.Int(postgresLeafValidityHours),
			EarlyRenewalHours:   pulumi.Int(postgresLeafRenewalHours),
			SetSubjectKeyId:     pulumi.Bool(true),
			AllowedUses: pulumi.StringArray{
				pulumi.String("digitalSignature"),
				pulumi.String(allowedUse),
			},
		})
		if err != nil {
			return nil, err
		}
		return &leaf{certificate: certificate.CertPem, privateKey: key.PrivateKeyPem}, nil
	}

	server, err := issueLeaf("postgres-server", "kamori-db-primary", "serverAuth", pulumi.StringArray{
		pulumi.String(databasePrimaryPrivateIP),
	}, nil)
	if err != nil {
		return nil, fmt.Errorf("create PostgreSQL server identity: %w", err)
	}
	app, err := issueLeaf("postgres-app-client", databaseApplicationRole, "clientAuth", nil, pulumi.StringArray{
		pulumi.String("spiffe://kamori.app/postgres/app"),
	})
	if err != nil {
		return nil, fmt.Errorf("create PostgreSQL app identity: %w", err)
	}
	jobs, err := issueLeaf("postgres-jobs-client", "kamori_jobs", "clientAuth", nil, pulumi.StringArray{
		pulumi.String("spiffe://kamori.app/postgres/jobs"),
	})
	if err != nil {
		return nil, fmt.Errorf("create PostgreSQL jobs identity: %w", err)
	}

	return &postgresPKI{
		caCertificate:         caCert.CertPem,
		serverCertificate:     server.certificate,
		serverPrivateKey:      server.privateKey,
		appClientCertificate:  app.certificate,
		appClientPrivateKey:   app.privateKey,
		jobsClientCertificate: jobs.certificate,
		jobsClientPrivateKey:  jobs.privateKey,
	}, nil
}

func provisionSSHPKI(ctx *pulumi.Context, hostNames []string) (*sshPKI, error) {
	caKey, err := tls.NewPrivateKey(ctx, "ssh-host-ca-key", &tls.PrivateKeyArgs{
		Algorithm: pulumi.String("ED25519"),
	})
	if err != nil {
		return nil, fmt.Errorf("create SSH host CA: %w", err)
	}
	deployKey, err := tls.NewPrivateKey(ctx, "ssh-deploy-key", &tls.PrivateKeyArgs{
		Algorithm: pulumi.String("ED25519"),
	})
	if err != nil {
		return nil, fmt.Errorf("create SSH deploy key: %w", err)
	}

	hosts := make(map[string]sshHostIdentity, len(hostNames))
	for index, hostName := range hostNames {
		hostKey, err := tls.NewPrivateKey(ctx, "ssh-host-key-"+hostName, &tls.PrivateKeyArgs{
			Algorithm: pulumi.String("ED25519"),
		})
		if err != nil {
			return nil, fmt.Errorf("create SSH host key for %s: %w", hostName, err)
		}
		certificate := pulumi.All(caKey.PrivateKeyOpenssh, hostKey.PublicKeyOpenssh).ApplyT(func(values []interface{}) (string, error) {
			return signSSHHostCertificate(values[0].(string), values[1].(string), hostName, uint64(index+1))
		}).(pulumi.StringOutput)
		hosts[hostName] = sshHostIdentity{
			privateKey:  hostKey.PrivateKeyOpenssh,
			certificate: certificate,
		}
	}

	return &sshPKI{
		caPublicKey:      caKey.PublicKeyOpenssh,
		deployPublicKey:  deployKey.PublicKeyOpenssh,
		deployPrivateKey: deployKey.PrivateKeyOpenssh,
		hosts:            hosts,
	}, nil
}

func signSSHHostCertificate(caPrivateKey, hostPublicKey, principal string, serial uint64) (string, error) {
	caSigner, err := ssh.ParsePrivateKey([]byte(caPrivateKey))
	if err != nil {
		return "", fmt.Errorf("parse SSH host CA key: %w", err)
	}
	hostKey, _, _, _, err := ssh.ParseAuthorizedKey([]byte(hostPublicKey))
	if err != nil {
		return "", fmt.Errorf("parse SSH host public key: %w", err)
	}
	certificate := &ssh.Certificate{
		Key:             hostKey,
		Serial:          serial,
		CertType:        ssh.HostCert,
		KeyId:           principal,
		ValidPrincipals: []string{principal},
		ValidAfter:      0,
		ValidBefore:     ssh.CertTimeInfinity,
	}
	if err := certificate.SignCert(bytes.NewReader(sshCertificateEntropy(hostPublicKey, principal)), caSigner); err != nil {
		return "", fmt.Errorf("sign SSH host certificate: %w", err)
	}
	return string(ssh.MarshalAuthorizedKey(certificate)), nil
}

func sshCertificateEntropy(hostPublicKey, principal string) []byte {
	digest := sha256.Sum256([]byte("kamori:ssh-host-certificate:v1\x00" + principal + "\x00" + hostPublicKey))
	return digest[:]
}
