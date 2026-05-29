import { render } from 'preact';
import { App } from './app';
import { ThemeProvider } from './hooks/useTheme';
import { LocaleProvider } from './hooks/useLocale';
import './index.css';

const root = document.getElementById('app');
if (root) {
  render(
    <LocaleProvider>
      <ThemeProvider>
        <App />
      </ThemeProvider>
    </LocaleProvider>,
    root,
  );
}
