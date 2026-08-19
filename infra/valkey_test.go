package main

import "testing"

func TestValkeyConnectionURLDerivesTopologyAndEscapesPassword(t *testing.T) {
	got := valkeyConnectionURL("cache@:/?#[]% word")
	want := "redis://:cache%40%3A%2F%3F%23%5B%5D%25%20word@10.42.0.31:6379/0"
	if got != want {
		t.Fatalf("valkeyConnectionURL() = %q, want %q", got, want)
	}
}
