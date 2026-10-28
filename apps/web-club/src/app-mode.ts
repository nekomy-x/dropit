declare global {
  const __APP_MODE__: 'demo' | 'production';
}

export const APP_MODE = __APP_MODE__;
export const IS_DEMO = APP_MODE === 'demo';
