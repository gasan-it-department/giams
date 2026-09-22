import { useQuery, type UseQueryResult } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Amount } from '@/lib/money'

export const bookKeys = {
  all:  ['books'] as const,
  book: (book: string, from: string, to: string, fundId?: string) =>
    [...bookKeys.all, book, from, to, fundId ?? 'all'] as const,
  trialBalance: (asOf: string, fundId?: string) =>
    [...bookKeys.all, 'tb', asOf, fundId ?? 'all'] as const,
}

/** The four books of original entry, by their COA annex. */
export const BOOKS = [
  { key: 'GJ',   annex: 'Annex 1',  label: 'General Journal' },
  { key: 'CRJ',  annex: 'Annex 2',  label: 'Cash Receipts' },
  { key: 'CKDJ', annex: 'Annex 3',  label: 'Check Disbursements' },
  { key: 'CDJ',  annex: 'Annex 4',  label: 'Cash Disbursements' },
] as const

export interface BookLine {
  entry_date:   string
  jev_no:       string
  particulars:  string
  reference:    string | null
  counterparty: string | null
  account_code: string
  account_name: string
  debit:        Amount
  credit:       Amount
}

export interface BookPage {
  annex:        string
  title:        string
  municipality: string
  fund_name:    string
  period_label: string
  lines:        BookLine[]
  total_debit:  Amount
  total_credit: Amount
}

export function useBook(
  book: string,
  from: string,
  to: string,
  fundId?: string,
): UseQueryResult<BookPage, Error> {
  return useQuery({
    queryKey: bookKeys.book(book, from, to, fundId),
    queryFn:  () =>
      api.get<BookPage>(
        `/api/books/${book}?from=${from}&to=${to}${fundId ? `&fund_id=${fundId}` : ''}`,
      ),
    enabled: book !== 'TB',
  })
}

export interface TrialBalanceLine {
  account_code:   string
  account_name:   string
  account_type:   string
  debit_balance:  Amount
  credit_balance: Amount
}

export interface TrialBalancePage {
  municipality: string
  fund_name:    string
  as_of:        string
  lines:        TrialBalanceLine[]
  total_debit:  Amount
  total_credit: Amount
  /** Zero when the ledger balances. Anything else must not be filed. */
  difference:   Amount
}

export function useTrialBalance(
  asOf: string,
  fundId?: string,
  enabled = true,
): UseQueryResult<TrialBalancePage, Error> {
  return useQuery({
    queryKey: bookKeys.trialBalance(asOf, fundId),
    queryFn:  () =>
      api.get<TrialBalancePage>(
        `/api/trial-balance?as_of=${asOf}${fundId ? `&fund_id=${fundId}` : ''}`,
      ),
    enabled,
  })
}
