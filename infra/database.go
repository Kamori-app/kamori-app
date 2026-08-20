package main

import "net/url"

const (
	databasePrimaryPrivateIP = "10.42.0.21"
	databasePort             = "5432"
	databaseName             = "kamori"
	databaseApplicationRole  = "kamori_app"
)

func databaseConnectionURL(password string) string {
	credentials := url.UserPassword(databaseApplicationRole, password).String()
	return "postgres://" + credentials + "@" + databasePrimaryPrivateIP + ":" + databasePort + "/" + databaseName +
		"?sslmode=verify-full" +
		"&sslrootcert=/run/secrets/postgres-ca.crt" +
		"&sslcert=/run/secrets/postgres-client.crt" +
		"&sslkey=/run/secrets/postgres-client.key"
}
