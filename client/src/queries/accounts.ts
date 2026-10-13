import {
  useMutation, useQuery, useQueryClient,
  type UseMutationResult, type UseQueryResult,
} from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Account, AccountFilter, AccountType, NewAccount } from '@/schemas/accounts'

// ── Query keys ────────────────────────────────────────────────────────────
// The filter is part of the key, so changing a search term is a separate
// cache entry rather than an invalidation.
export const accountKeys = {
  all:      ['accounts'] as const,
  lists:    () => [...accountKeys.all, 'list'] as const,
  list:     (filter: AccountFilter) => [...accountKeys.lists(), filter] as const,
  funds:    () => ['funds'] as const,
}

function toSearchParams(filter: AccountFilter): string {
  const params = new URLSearchParams()
  if (filter.search)          params.set('search', filter.search)
  if (filter.account_type)    params.set('account_type', filter.account_type)
  if (filter.postable_only)   params.set('postable_only', 'true')
  if (filter.include_inactive) params.set('include_inactive', 'true')
  if (filter.page)             params.set('page', String(filter.page))
  const q = params.toString()
  return q ? `?${q}` : ''
}

export interface Page<T> {
  items: T[]
  total: number
}

/** Rows per page. Must match `PAGE_SIZE` on the server. */
export const PAGE_SIZE = 20

export function useAccounts(filter: AccountFilter = {}): UseQueryResult<Page<Account>, Error> {
  return useQuery({
    queryKey: accountKeys.list(filter),
    queryFn:  () => api.get<Page<Account>>(`/api/accounts${toSearchParams(filter)}`),
    // Keeps the previous page on screen while the next loads, so paging does
    // not blink through an empty list.
    placeholderData: (previous) => previous,
    // The chart changes rarely; a minute of staleness saves a request on
    // every navigation back to this screen.
    staleTime: 60_000,
  })
}

/** An account as the pickers show it. */
export interface AccountOption {
  id:            string
  code:          string
  name:          string
  account_type:  AccountType
  expense_class: string | null
  /** The summary account above it, e.g. "Cash" over "Cash - Local Treasury". */
  parent_name:   string | null
}

/**
 * Every account an entry can be posted to, unpaged.
 *
 * Loaded once and filtered where it is shown, so the picker lists the whole
 * chart the moment it opens and narrows as the clerk types, with no request
 * per keystroke. Also the only safe source for looking a code up by id: a
 * lookup against one page of the paged list silently misses the rest.
 */
export function useAccountOptions(accountType?: AccountType): UseQueryResult<AccountOption[], Error> {
  return useQuery({
    queryKey:  [...accountKeys.all, 'options', accountType ?? 'all'],
    queryFn:   () => api.get<AccountOption[]>(
      `/api/accounts/options${accountType ? `?account_type=${accountType}` : ''}`,
    ),
    staleTime: 5 * 60_000,
  })
}

export interface Fund {
  id: string
  code: string
  name: string
  kind: string
  parent_fund_id: string | null
  is_statutory_subfund: boolean
  is_active: boolean
}

export function useFunds(): UseQueryResult<Fund[], Error> {
  return useQuery({
    queryKey:  accountKeys.funds(),
    queryFn:   () => api.get<Fund[]>('/api/funds'),
    staleTime: 5 * 60_000,
  })
}

export interface Office {
  id: string
  code: string
  name: string
}

export function useOffices(): UseQueryResult<Office[], Error> {
  return useQuery({
    queryKey:  ['offices'],
    queryFn:   () => api.get<Office[]>('/api/offices'),
    staleTime: 5 * 60_000,
  })
}

export function useCreateAccount(): UseMutationResult<Account, Error, NewAccount> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (input: NewAccount) => api.post<Account>('/api/accounts', input),
    onSuccess: () => {
      // Every filtered list is now potentially wrong, so the whole branch goes.
      void queryClient.invalidateQueries({ queryKey: accountKeys.lists() })
    },
  })
}

export function useDeactivateAccount(): UseMutationResult<unknown, Error, string> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: string) => api.del(`/api/accounts/${id}`),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: accountKeys.lists() })
    },
  })
}
