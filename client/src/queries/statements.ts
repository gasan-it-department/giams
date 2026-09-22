import { useQuery, type UseQueryResult } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Amount } from '@/lib/money'

/**
 * The general purpose financial statements.
 *
 * Every figure here is computed by the database from posted entries. Nothing
 * is totalled in the browser: a statement that disagreed with the ledger it
 * summarises would be wrong in the worst way — silently, and for however long
 * nobody checked.
 */

export const statementKeys = {
  all:         ['statements'] as const,
  position:    (asOf: string, fundId?: string) =>
    [...statementKeys.all, 'position', asOf, fundId ?? 'all'] as const,
  performance: (from: string, to: string, fundId?: string) =>
    [...statementKeys.all, 'performance', from, to, fundId ?? 'all'] as const,
  budget:      (year: number, fundId?: string) =>
    [...statementKeys.all, 'budget', year, fundId ?? 'all'] as const,
}

const fund = (fundId?: string) => (fundId ? `&fund_id=${fundId}` : '')

// ── Statement of Financial Position ──────────────────────────────────────────

export interface StatementLine {
  section:      string
  account_code: string
  account_name: string
  amount:       Amount
  /** Totals and sub-totals are ruled off from the detail above them. */
  is_total:     boolean
}

export interface FinancialPosition {
  municipality: string
  fund_name:    string
  as_of:        string
  lines:        StatementLine[]
  /** Assets less liabilities and equity. Zero when the statement balances. */
  check:        Amount
}

export function useFinancialPosition(
  asOf: string,
  fundId?: string,
  enabled = true,
): UseQueryResult<FinancialPosition, Error> {
  return useQuery({
    queryKey: statementKeys.position(asOf, fundId),
    queryFn:  () =>
      api.get<FinancialPosition>(
        `/api/statements/financial-position?as_of=${asOf}${fund(fundId)}`,
      ),
    enabled,
  })
}

// ── Statement of Financial Performance ───────────────────────────────────────

export interface PerformanceLine {
  section:       string
  account_code:  string
  account_name:  string
  expense_class: string | null
  amount:        Amount
  is_total:      boolean
}

export interface FinancialPerformance {
  municipality: string
  fund_name:    string
  period_label: string
  lines:        PerformanceLine[]
}

export function useFinancialPerformance(
  from: string,
  to: string,
  fundId?: string,
  enabled = true,
): UseQueryResult<FinancialPerformance, Error> {
  return useQuery({
    queryKey: statementKeys.performance(from, to, fundId),
    queryFn:  () =>
      api.get<FinancialPerformance>(
        `/api/statements/financial-performance?from=${from}&to=${to}${fund(fundId)}`,
      ),
    enabled,
  })
}

// ── Comparison of Budget and Actual Amounts ──────────────────────────────────

export interface BudgetActualLine {
  expense_class:    string
  office_name:      string
  account_code:     string
  account_name:     string
  original_budget:  Amount
  final_budget:     Amount
  obligated:        Amount
  disbursed:        Amount
  unused_budget:    Amount
  /** A percentage, not money. */
  utilisation_rate: string
}

export interface BudgetActual {
  municipality: string
  fund_name:    string
  fiscal_year:  number
  lines:        BudgetActualLine[]
}

export function useBudgetActual(
  fiscalYear: number,
  fundId?: string,
  enabled = true,
): UseQueryResult<BudgetActual, Error> {
  return useQuery({
    queryKey: statementKeys.budget(fiscalYear, fundId),
    queryFn:  () =>
      api.get<BudgetActual>(
        `/api/statements/budget-vs-actual?fiscal_year=${fiscalYear}${fund(fundId)}`,
      ),
    enabled,
  })
}
