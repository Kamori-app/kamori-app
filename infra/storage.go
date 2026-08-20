package main

import (
	"fmt"
	"regexp"
	"strings"
)

const (
	backblazeRegion               = "eu-central-003"
	backblazeEndpoint             = "s3." + backblazeRegion + ".backblazeb2.com"
	backblazePrimaryBucket        = "kamori-production-primary"
	backblazePostgresBackupBucket = "kamori-production-postgres"
	hetznerObjectLocation         = "fsn1"
	hetznerObjectEndpoint         = hetznerObjectLocation + ".your-objectstorage.com"
	hetznerObjectS3CompatMode     = true
)

var s3BucketNamePattern = regexp.MustCompile(`^[a-z0-9][a-z0-9-]{1,61}[a-z0-9]$`)

func hetznerDRBucketName(stack string) (string, error) {
	name := "kamori-app-" + strings.ToLower(stack) + "-dr"
	if !s3BucketNamePattern.MatchString(name) {
		return "", fmt.Errorf("derived Hetzner DR bucket name %q is not a valid S3 bucket name; use a lowercase alphanumeric or hyphenated Pulumi stack name", name)
	}
	return name, nil
}
