package main

import "fmt"

const decimalTerabyte = int64(1_000_000_000_000)

type BudgetGuardrails struct {
	BetaAccounts               int   `json:"betaAccounts"`
	OwnerMonthlyEgressBytes    int64 `json:"ownerMonthlyEgressBytes"`
	OwnerRolling24hEgressBytes int64 `json:"ownerRolling24hEgressBytes"`
	IssuedEgressBytes          int64 `json:"issuedEgressBytes"`
	WarnBytes                  int64 `json:"warnBytes"`
	ThrottleBytes              int64 `json:"throttleBytes"`
	NonessentialStopBytes      int64 `json:"nonessentialStopBytes"`
	CriticalWarnBytes          int64 `json:"criticalWarnBytes"`
	EmergencyBreakerBytes      int64 `json:"emergencyBreakerBytes"`
	CoreReservedBytes          int64 `json:"coreReservedBytes"`
}

func defaultBudgetGuardrails() BudgetGuardrails {
	return BudgetGuardrails{
		BetaAccounts:               1_000,
		OwnerMonthlyEgressBytes:    10_000_000_000,
		OwnerRolling24hEgressBytes: 2_000_000_000,
		IssuedEgressBytes:          10 * decimalTerabyte,
		WarnBytes:                  10 * decimalTerabyte,
		ThrottleBytes:              14 * decimalTerabyte,
		NonessentialStopBytes:      16 * decimalTerabyte,
		CriticalWarnBytes:          18 * decimalTerabyte,
		EmergencyBreakerBytes:      19 * decimalTerabyte,
		CoreReservedBytes:          4 * decimalTerabyte,
	}
}

func (guardrails BudgetGuardrails) validate() error {
	if guardrails.BetaAccounts <= 0 {
		return fmt.Errorf("beta account admission cap must be positive")
	}
	if guardrails.OwnerMonthlyEgressBytes <= 0 || guardrails.OwnerRolling24hEgressBytes <= 0 {
		return fmt.Errorf("owner egress quotas must be positive")
	}
	if guardrails.IssuedEgressBytes > guardrails.WarnBytes {
		return fmt.Errorf("issued egress must not exceed the first infrastructure warning")
	}
	if !(guardrails.WarnBytes < guardrails.ThrottleBytes &&
		guardrails.ThrottleBytes < guardrails.NonessentialStopBytes &&
		guardrails.NonessentialStopBytes < guardrails.CriticalWarnBytes &&
		guardrails.CriticalWarnBytes < guardrails.EmergencyBreakerBytes) {
		return fmt.Errorf("global egress thresholds must be strictly increasing")
	}
	if guardrails.CoreReservedBytes < guardrails.EmergencyBreakerBytes-guardrails.NonessentialStopBytes {
		return fmt.Errorf("core reserve must cover the emergency interval")
	}
	return nil
}
