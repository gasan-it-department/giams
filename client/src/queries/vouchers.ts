import {
  useMutation, useQuery, useQueryClient,
  type UseMutationResult, type UseQueryResult,
} from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Amount } from '@/lib/money'
import { budgetKeys } from '@/queries/budget'

export const voucherKeys = {
  all:        ['vouchers'] as const,
  list:       (filter: VoucherFilter) => [...voucherKeys.all, 'list', filter] as const,
  deductions: (id: string) => [...voucherKeys.all, id, 'deductions'] as const,
  types:      (appliesTo: string) => ['deduction-types', appliesTo] as const,
}

export interface Voucher {
  id:                string
  dv_no:             string
  fund_name:         string
  fiscal_year:       number
  dv_date:           string
  particulars:       string
  gross_amount:      Amount
  total_deductions:  Amount
  net_amount:        Amount
  paid_amount:       Amount
  status:            string
  payee_name:        string
  office_name:       string
  ors_no:            string
  obligation_id:     string
  prepared_by_name:  string | null
  approved_by_name:  string | null
  posted:            boolean
}

export interface VoucherDeduction {
  code:            string
  name:            string
  remit_to:        string
  base:            Amount
  rate_applied:    string | null
  amount_deducted: Amount
  authority:       string | null
}

export interface DeductionOption {
  id:       string
  code:     string
  name:     string
  remit_to: string
  basis:    string
}

export interface VoucherFilter {
  fiscal_year?: number
  status?:      string
  search?:      string
}

export type VoucherStep = 'submit' | 'certify' | 'approve'

function query(filter: object): string {
  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(filter)) {
    if (value !== undefined && value !== null && value !== '') params.set(key, String(value))
  }
  const q = params.toString()
  return q ? `?${q}` : ''
}

export function useVouchers(filter: VoucherFilter = {}): UseQueryResult<Voucher[], Error> {
  return useQuery({
    queryKey: voucherKeys.list(filter),
    queryFn:  () => api.get<Voucher[]>(`/api/vouchers${query(filter)}`),
  })
}

/**
 * The deduction lines the server worked out.
 *
 * Fetched per voucher and only when its detail is opened — a register of 300
 * vouchers should not pull every deduction line to render a list.
 */
export function useVoucherDeductions(
  id: string | null,
): UseQueryResult<VoucherDeduction[], Error> {
  return useQuery({
    queryKey: voucherKeys.deductions(id ?? ''),
    queryFn:  () => api.get<VoucherDeduction[]>(`/api/vouchers/${id}/deductions`),
    enabled:  id !== null,
    // Deductions are fixed once computed; there is nothing to re-poll for.
    staleTime: Infinity,
  })
}

export function useDeductionTypes(appliesTo = 'supplier'): UseQueryResult<DeductionOption[], Error> {
  return useQuery({
    queryKey:  voucherKeys.types(appliesTo),
    queryFn:   () => api.get<DeductionOption[]>(`/api/deduction-types?applies_to=${appliesTo}`),
    staleTime: 5 * 60_000,
  })
}

/**
 * Moves a voucher one step along, or posts it.
 *
 * Both invalidate the budget branch too: a posted voucher changes the
 * obligation's payable figure, which the RAOD registry reports.
 */
export function useAdvanceVoucher(): UseMutationResult<
  Voucher, Error, { id: string; step: VoucherStep }
> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, step }) => api.post<Voucher>(`/api/vouchers/${id}/advance`, { step }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: voucherKeys.all })
      void queryClient.invalidateQueries({ queryKey: budgetKeys.all })
    },
  })
}

export function usePostVoucher(): UseMutationResult<Voucher, Error, string> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: string) => api.post<Voucher>(`/api/vouchers/${id}/post`, {}),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: voucherKeys.all })
      void queryClient.invalidateQueries({ queryKey: budgetKeys.all })
    },
  })
}
