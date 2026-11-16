import { Component, type ReactNode } from 'react'

/**
 * Keeps a crash inside the spreadsheet editor inside it.
 *
 * The editor throws while drawing on some files it can't handle; without this
 * the whole app goes blank. With it, the rest of GIAMS stays usable and the
 * person is told what to do.
 */
export class EditorBoundary extends Component<
  { onReset: () => void; children: ReactNode },
  { failed: boolean }
> {
  state = { failed: false }

  static getDerivedStateFromError() {
    return { failed: true }
  }

  componentDidCatch(error: unknown) {
    console.error('[spreadsheets] the editor crashed', error)
  }

  render() {
    if (!this.state.failed) return this.props.children

    return (
      <div className="px-6 py-12 text-center">
        <p className="font-medium">This spreadsheet could not be shown.</p>
        <p className="mx-auto mt-1 max-w-[56ch] text-ink-secondary">
          Something in the file is more than this editor can handle. The file itself is unchanged. Open it in Excel instead, or try another file.
        </p>
        <button
          type="button"
          onClick={() => { this.setState({ failed: false }); this.props.onReset() }}
          className="mt-4 rounded-pill bg-accent-fill px-4 py-2 font-medium text-accent-ink hover:brightness-[0.97]"
        >
          Close this file
        </button>
      </div>
    )
  }
}
