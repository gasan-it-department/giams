import { AlertTriangle, Clock } from 'lucide-react'
import { Link } from 'react-router-dom'
import { useLicence } from '@/queries/licence'

/**
 * The subscription notice, across the top of every screen.
 *
 * Shown only when there is something to say. A banner that is always there is
 * a banner nobody reads, and this one has to be read on the day it matters.
 *
 * The wording comes from the server, which phrases it once for the whole
 * system. Two copies of a message like this drift, and the day they differ is
 * the day somebody is told two different things about why they cannot work.
 */
export function LicenceBanner() {
  const { data } = useLicence()

  if (!data?.notice) return null

  const serious = data.state === 'read_only' || data.clock_problem
  const Icon = data.clock_problem ? Clock : AlertTriangle

  return (
    <div
      role="status"
      className={`flex items-start gap-2.5 border-b border-line px-4 py-2 text-sm ${
        serious ? 'bg-danger-fill text-danger-ink' : 'bg-warn-fill text-warn-ink'
      }`}
    >
      <Icon className="mt-0.5 size-4 shrink-0" aria-hidden />
      <p className="grow">
        {data.notice}{' '}
        <Link to="/system" className="underline underline-offset-2">
          Subscription details
        </Link>
      </p>
    </div>
  )
}
