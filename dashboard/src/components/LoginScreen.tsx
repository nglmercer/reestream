import { useState } from 'preact/hooks';
import { apiV1 } from '../api';
import { useLocale } from '../hooks/useLocale';

interface Props {
  onAuthenticated: () => void;
}

export function LoginScreen({ onAuthenticated }: Props) {
  const { t } = useLocale();
  const [email, setEmail] = useState('admin@localhost');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const submit = async (event: Event) => {
    event.preventDefault();
    setSubmitting(true);
    setError(null);
    try {
      await apiV1.login(email.trim(), password);
      onAuthenticated();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : t('auth.invalidCredentials'));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div class="setup-shell min-h-screen bg-surface flex items-center justify-center p-4">
      <div class="setup-frame w-full max-w-md">
        <div class="setup-card bg-surface-alt border border-border rounded-2xl p-8">
          <div class="text-center mb-7">
            <div class="brand-mark mx-auto mb-4"><span>✦</span></div>
            <h1 class="text-2xl font-bold text-fg">{t('auth.title')}</h1>
            <p class="text-fg-muted text-sm mt-2">{t('auth.description')}</p>
          </div>
          <form onSubmit={submit} class="space-y-4">
            <label class="block text-sm text-fg-muted">
              {t('auth.email')}
              <input
                class="mt-1 w-full rounded-lg border border-border bg-surface-hover px-3 py-2 text-fg"
                type="email"
                value={email}
                autocomplete="username"
                onInput={(event) => setEmail((event.currentTarget as HTMLInputElement).value)}
                required
              />
            </label>
            <label class="block text-sm text-fg-muted">
              {t('auth.password')}
              <input
                class="mt-1 w-full rounded-lg border border-border bg-surface-hover px-3 py-2 text-fg"
                type="password"
                value={password}
                autocomplete="current-password"
                onInput={(event) => setPassword((event.currentTarget as HTMLInputElement).value)}
                required
              />
            </label>
            {error && <p class="rounded-lg border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">{error}</p>}
            <button
              class="w-full rounded-lg bg-accent px-4 py-2.5 font-semibold text-white disabled:opacity-50"
              type="submit"
              disabled={submitting}
            >
              {submitting ? t('auth.signingIn') : t('auth.signIn')}
            </button>
          </form>
        </div>
      </div>
    </div>
  );
}
