package main

import "testing"

func TestDefaultBudgetGuardrailsAreInternallyConsistent(t *testing.T) {
	t.Parallel()
	if err := defaultBudgetGuardrails().validate(); err != nil {
		t.Fatalf("default guardrails must validate: %v", err)
	}
}

func TestBudgetGuardrailsRejectOverissuedQuota(t *testing.T) {
	t.Parallel()
	guardrails := defaultBudgetGuardrails()
	guardrails.IssuedEgressBytes = guardrails.WarnBytes + 1
	if err := guardrails.validate(); err == nil {
		t.Fatal("overissued egress must be rejected")
	}
}
