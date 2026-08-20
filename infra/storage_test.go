package main

import "testing"

func TestBackblazeTopologyIsVersioned(t *testing.T) {
	if backblazeRegion != "eu-central-003" {
		t.Fatalf("Backblaze region = %q", backblazeRegion)
	}
	if backblazeEndpoint != "s3.eu-central-003.backblazeb2.com" {
		t.Fatalf("Backblaze endpoint = %q", backblazeEndpoint)
	}
	if backblazePrimaryBucket != "kamori-production-primary" {
		t.Fatalf("Backblaze primary bucket = %q", backblazePrimaryBucket)
	}
	if backblazePostgresBackupBucket != "kamori-production-postgres" {
		t.Fatalf("Backblaze PostgreSQL bucket = %q", backblazePostgresBackupBucket)
	}
}

func TestHetznerObjectStorageTopologyIsDerived(t *testing.T) {
	if hetznerObjectLocation != "fsn1" {
		t.Fatalf("Hetzner Object Storage location = %q, want fsn1", hetznerObjectLocation)
	}
	if hetznerObjectEndpoint != "fsn1.your-objectstorage.com" {
		t.Fatalf("Hetzner Object Storage endpoint = %q", hetznerObjectEndpoint)
	}
	if !hetznerObjectS3CompatMode {
		t.Fatal("Hetzner Object Storage must use the provider's S3 compatibility mode")
	}
	bucket, err := hetznerDRBucketName("production")
	if err != nil {
		t.Fatal(err)
	}
	if bucket != "kamori-app-production-dr" {
		t.Fatalf("Hetzner DR bucket = %q, want kamori-app-production-dr", bucket)
	}
}

func TestHetznerDRBucketRejectsInvalidStackName(t *testing.T) {
	if _, err := hetznerDRBucketName("Production_US"); err == nil {
		t.Fatal("expected an invalid S3 bucket name error")
	}
}
