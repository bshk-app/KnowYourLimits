import { vi } from 'vitest';

// Text assertions use English regardless of the host's Windows/macOS language.
vi.stubGlobal('navigator', { languages: ['en-US'], language: 'en-US' });
