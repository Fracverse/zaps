import { Platform } from "react-native";
import NfcManager, { NfcEvents, NfcTech } from "react-native-nfc-manager";

export interface NfcTapToPayResult {
  raw: string;
}

export interface NfcTapToPayOptions {
  timeoutMs?: number;
  onScan?: (raw: string) => void;
}

function timeoutPromise(ms: number, message: string) {
  return new Promise<never>((_, reject) => {
    const id = setTimeout(() => {
      clearTimeout(id);
      reject(new Error(message));
    }, ms);
  });
}

// Reads an NFC tag and returns a best-effort raw string.
// Note: react-native-nfc-manager returns technology-specific payloads.
// This implementation tries common text/URI payloads.
export async function scanNfcTag(
  options: NfcTapToPayOptions = {}
): Promise<NfcTapToPayResult> {
  const { timeoutMs = 15000, onScan } = options;

  if (Platform.OS === "ios") {
    // iOS requires NFC formats/entitlements; still attempt gracefully.
  }

  await NfcManager.start();

  try {
    const hasNfc = await NfcManager.isSupported();
    if (!hasNfc) {
      throw new Error("NFC is not supported on this device");
    }

    const isEnabled = await NfcManager.isEnabled();
    if (!isEnabled) {
      throw new Error("NFC is disabled. Please enable NFC");
    }

    return await Promise.race([
      new Promise<NfcTapToPayResult>((resolve, reject) => {
        const cleanup = async () => {
          try {
            await NfcManager.cancelTechnologyRequest();
          } catch {
            // ignore
          }
        };

        const onTagDiscovered = async (tag: any) => {
          try {
            // Best-effort: try to surface something meaningful.
            // tag?.ndefMessage typically exists for NDEF tags.
            let raw = "";

            // react-native-nfc-manager docs: tag.ndefMessage[0].payload
            const ndefMsg = tag?.ndefMessage;
            if (Array.isArray(ndefMsg) && ndefMsg[0]?.payload) {
              const payload = ndefMsg[0].payload;
              // payload might be a byte array; attempt string.
              // Many NDEF text records encode language code first.
              if (Array.isArray(payload)) {
                raw = Buffer.from(payload).toString("utf8");
              } else if (typeof payload === "string") {
                raw = payload;
              }
            }

            raw = raw.trim();
            if (!raw) {
              // Try tech-specific tag data
              const techPayload =
                tag?.id && typeof tag.id === "string" ? tag.id : "";
              raw = techPayload || JSON.stringify(tag);
            }

            onScan?.(raw);
            resolve({ raw });
          } catch (e) {
            reject(e as Error);
          } finally {
            cleanup();
          }
        };

        NfcManager.setEventListener(NfcEvents.DiscoverTag, onTagDiscovered);

        // Request a generic NDEF technology; manager will choose supported.
        NfcManager.requestTechnology(NfcTech.Ndef).catch(reject);
      }),

      timeoutPromise(timeoutMs, `NFC scan timed out after ${timeoutMs}ms`),
    ]);
  } finally {
    // Always stop after a run.
    try {
      await NfcManager.cancelTechnologyRequest();
    } catch {
      // ignore
    }
    try {
      await NfcManager.setEventListener(NfcEvents.DiscoverTag, () => {});
    } catch {
      // ignore
    }
  }
}
