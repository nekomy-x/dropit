// Temporary isolated entry for the connect landing, used while unrelated
// in-progress work breaks App.tsx's import graph. Not part of the product.
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router-dom';
import StartPage from './pages/connect/StartPage';
import { LocaleProvider } from './locale';
import './index.css';
import './operations.css';

const rootElement = document.getElementById('root');
if (!rootElement) throw new Error('Root element #root not found');

createRoot(rootElement).render(
  <StrictMode>
    <BrowserRouter>
      <LocaleProvider>
        <StartPage />
      </LocaleProvider>
    </BrowserRouter>
  </StrictMode>,
);
