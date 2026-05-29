import { render } from 'preact';
import { App } from './app';
import { ThemeProvider } from './hooks/useTheme';
import './index.css';

const root = document.getElementById('app');
if (root) {
  render(
    <ThemeProvider>
      <App />
    </ThemeProvider>,
    root,
  );
}
