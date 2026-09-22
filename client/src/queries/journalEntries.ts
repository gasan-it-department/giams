import { useMutation, useQueryClient, type UseMutationResult } from '@tanstack/react-query'
import { api } from '@/lib/api'
import type { Amount } from '@/lib/money'
import { bookKeys } from '@/queries/books'

export interface NewEntryLine {
  account_id:  string
  debit:       Amount
  credit:      Amount
  particulars?: string
}

export interface NewJournalEntry {
  fund_id:      string
  entry_date:   string
  journal_type: string
  particulars:  string
  lines:        NewEntryLine[]
}

export interface SavedEntry {
  id:      string
  jev_no:  string
  message: string
}

export function useCreateJournalEntry(): UseMutationResult<SavedEntry, Error, NewJournalEntry> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (entry: NewJournalEntry) => api.post<SavedEntry>('/api/journal-entries', entry),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: bookKeys.all })
    },
  })
}

export interface PostedEntry {
  posted:  boolean
  jev_no:  string
  message: string
}

export function usePostJournalEntry(): UseMutationResult<PostedEntry, Error, string> {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: string) => api.post<PostedEntry>(`/api/journal-entries/${id}/post`, {}),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: bookKeys.all })
    },
  })
}
