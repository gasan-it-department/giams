import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { BrowserRouter } from 'react-router-dom'
import App from './App'
import { ApiError } from '@/lib/api'
import './index.css'

/**
 * Retry policy.
 *
 * Only genuinely transient failures are retried. A rejected accounting rule --
 * an unbalanced entry, an obligation over its allotment, a closed period --
 * will be rejected identically every time, and retrying it just delays the
 * message the clerk needs to read.
 */
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: (failureCount, error) => {
        if (error instanceof ApiError && !error.isTransient) return false
        return failureCount < 2
      },
      retryDelay: (attempt) => Math.min(1000 * 2 ** attempt, 8000),
      refetchOnWindowFocus: false,
      staleTime: 30_000,
    },
    mutations: {
      // Never retry a mutation automatically. Re-sending a posting that may
      // have succeeded is exactly how a duplicate voucher gets created.
      retry: false,
    },
  },
})

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </QueryClientProvider>
  </StrictMode>,
)
