type Subscription = { remove: () => void };

let cachedModule: any;

function getModule(): any {
  if (cachedModule === undefined) {
    try {
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      cachedModule = require("expo-screen-capture");
    } catch {
      cachedModule = null;
    }
  }
  return cachedModule;
}

export async function preventScreenCaptureAsync(): Promise<void> {
  const mod = getModule();
  if (mod?.preventScreenCaptureAsync) {
    await mod.preventScreenCaptureAsync();
  }
}

export async function allowScreenCaptureAsync(): Promise<void> {
  const mod = getModule();
  if (mod?.allowScreenCaptureAsync) {
    await mod.allowScreenCaptureAsync();
  }
}

export function addScreenshotListener(listener: () => void): Subscription {
  const mod = getModule();
  if (mod?.addScreenshotListener) {
    return mod.addScreenshotListener(listener);
  }
  return { remove: () => {} };
}
