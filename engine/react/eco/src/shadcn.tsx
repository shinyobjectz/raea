// React 19 + Tailwind v4 + Radix primitives styled the shadcn way. The Dialog is open from the start:
// a portal into <body>, a fixed overlay, focus moved into the content.
import { createRoot } from "react-dom/client"
import { flushSync } from "react-dom"
import * as Tabs from "@radix-ui/react-tabs"
import * as Progress from "@radix-ui/react-progress"
import * as Avatar from "@radix-ui/react-avatar"
import * as Dialog from "@radix-ui/react-dialog"

const people = [["AN", "Ana"], ["BE", "Ben"], ["CH", "Chi"]]

function App() {
  return (
    <div className="p-8 font-sans text-slate-900">
      <div className="max-w-xl rounded-xl border border-slate-200 bg-white p-6 shadow-sm">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-xl font-semibold tracking-tight">Launch plan</h2>
          <div className="flex -space-x-2">
            {people.map(([i, n]) => (
              <Avatar.Root key={i} className="inline-flex size-8 items-center justify-center overflow-hidden rounded-full border-2 border-white bg-slate-200">
                <Avatar.Fallback className="text-xs font-medium text-slate-700">{i}</Avatar.Fallback>
              </Avatar.Root>
            ))}
          </div>
        </div>
        <Tabs.Root defaultValue="tasks">
          <Tabs.List className="mb-4 inline-flex rounded-lg bg-slate-100 p-1">
            {["tasks", "files", "notes"].map((v) => (
              <Tabs.Trigger key={v} value={v} className="rounded-md px-3 py-1 text-sm font-medium text-slate-500 data-[state=active]:bg-white data-[state=active]:text-slate-900 data-[state=active]:shadow-sm">
                {v[0].toUpperCase() + v.slice(1)}
              </Tabs.Trigger>
            ))}
          </Tabs.List>
          <Tabs.Content value="tasks" className="space-y-3">
            {[["Script", 100], ["Shoot", 64], ["Edit", 20]].map(([label, v]) => (
              <div key={label as string}>
                <div className="mb-1 flex justify-between text-sm"><span>{label}</span><span className="text-slate-500">{v}%</span></div>
                <Progress.Root value={v as number} className="h-2 overflow-hidden rounded-full bg-slate-100">
                  <Progress.Indicator className="h-full bg-indigo-600" style={{ width: `${v}%` }} />
                </Progress.Root>
              </div>
            ))}
          </Tabs.Content>
          <Tabs.Content value="files">files</Tabs.Content>
        </Tabs.Root>
      </div>
      <Dialog.Root defaultOpen>
        <Dialog.Portal>
          <Dialog.Overlay className="fixed inset-0 bg-black/40" />
          <Dialog.Content className="fixed left-1/2 top-1/2 w-80 -translate-x-1/2 -translate-y-1/2 rounded-xl bg-white p-6 shadow-xl">
            <Dialog.Title className="text-lg font-semibold">Ship it?</Dialog.Title>
            <Dialog.Description className="mt-1 text-sm text-slate-500">Two tasks are still open.</Dialog.Description>
            <div className="mt-4 flex justify-end gap-2">
              <button className="rounded-md border border-slate-200 px-3 py-1.5 text-sm">Cancel</button>
              <button className="rounded-md bg-slate-900 px-3 py-1.5 text-sm text-white">Ship</button>
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </div>
  )
}

flushSync(() => createRoot(document.getElementById("app")!).render(<App />))
