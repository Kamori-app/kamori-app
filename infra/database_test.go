package main

import "testing"

func TestDatabaseConnectionURLDerivesTopologyAndEscapesPassword(t *testing.T) {
	got := databaseConnectionURL("p@ss:/?#[]% word")
	want := "postgres://kamori_app:p%40ss%3A%2F%3F%23%5B%5D%25%20word@10.42.0.21:5432/kamori" +
		"?sslmode=verify-full" +
		"&sslrootcert=/run/secrets/postgres-ca.crt" +
		"&sslcert=/run/secrets/postgres-client.crt" +
		"&sslkey=/run/secrets/postgres-client.key"
	if got != want {
		t.Fatalf("databaseConnectionURL() = %q, want %q", got, want)
	}
}
