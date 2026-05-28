interface Props {
  version: string;
}

export function Header({ version }: Props) {
  return (
    <header class="bg-slate-900 border-b border-slate-800 px-6 py-4 flex items-center justify-between">
      <h1 class="text-lg font-bold text-sky-400">Reestream Dashboard</h1>
      <span class="text-sm text-slate-500">v{version}</span>
    </header>
  );
}
