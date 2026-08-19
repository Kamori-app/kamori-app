package main

import "net/url"

const (
	valkeyPrivateIP = "10.42.0.31"
	valkeyPort      = "6379"
	valkeyDatabase  = "0"
)

func valkeyConnectionURL(password string) string {
	credentials := url.UserPassword("", password).String()
	return "redis://" + credentials + "@" + valkeyPrivateIP + ":" + valkeyPort + "/" + valkeyDatabase
}
