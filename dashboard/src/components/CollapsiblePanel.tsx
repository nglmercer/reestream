import type { ComponentChildren } from 'preact';
import { useState } from 'preact/hooks';
import { Icon } from './Icon';

interface Props {
  title: string;
  summary?: string;
  children: ComponentChildren;
  defaultOpen?: boolean;
  className?: string;
}

export function CollapsiblePanel({ title, summary, children, defaultOpen = true, className = '' }: Props) {
  const [open, setOpen] = useState(defaultOpen);

  return (
    <section class={`collapsible-panel ${open ? 'is-open' : 'is-collapsed'} ${className}`}>
      <button
        type="button"
        class="collapsible-trigger"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        <span>
          <strong>{title}</strong>
          {summary && <small>{summary}</small>}
        </span>
        <Icon name="chevronDown" size={16} />
      </button>
      {open && <div class="collapsible-panel-body">{children}</div>}
    </section>
  );
}
