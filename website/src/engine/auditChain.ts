/**
 * Hakille — Live Cryptographic Audit Journal Engine (`SHA-256` + `Ed25519` WebCrypto)
 *
 * Implements the core guarantee of `crates/hakille-audit`:
 *   "Le Journal d'Audit EST le produit."
 *
 * Every entry is canonically serialized, chained to `prevHash` via `SHA-256`,
 * and signed in real time inside the visitor's browser using `crypto.subtle`
 * (`Ed25519` where supported by the runtime, with deterministic `HMAC-SHA-256`
 * fallback transparently reported on older engines).
 *
 * Also models the 3-level `WhySheet` (`crates/hakille-design/src/why.rs`)
 * including interactive SPO fact invalidation ("Ce fait est faux").
 */

export interface SpoAuditFact {
  readonly id: string;
  readonly subject: string;
  readonly predicate: string;
  readonly object: string;
  readonly partition: 'Personal' | 'Professional' | 'Shared';
  readonly freshness: 'frais' | 'récent' | 'fané';
  readonly disputed: boolean;
}

export interface WhySheetData {
  readonly level1PlainSentence: string;
  readonly level2Facts: readonly SpoAuditFact[];
  readonly level3Technical: {
    readonly agentId: string;
    readonly biscuitCapabilities: readonly string[];
    readonly confidenceScore: number;
    readonly domainThreshold: number;
    readonly modelUsed: string;
    readonly networkBytesSent: number;
    readonly counterfactual: string;
  };
}

export interface SignedAuditEntry {
  readonly sequence: number;
  readonly id: string;
  readonly timestampIso: string;
  readonly actor: 'HumanConfirmed600ms' | 'ConstitutionalGuard' | 'ReversibleAutonomous' | 'HumanUndo';
  readonly trustOrigin: 'TrustedHuman' | 'UntrustedExternal';
  readonly title: string;
  readonly outcome: 'Sealed' | 'BlockedRule01' | 'Reverted';
  readonly reversible: boolean;
  readonly reverted: boolean;
  readonly prevHashHex: string;
  readonly entryHashHex: string;
  readonly signatureHex: string;
  readonly signatureScheme: 'Ed25519-WebCrypto' | 'HMAC-SHA256-WebCrypto';
  readonly why: WhySheetData;
}

export interface AuditKeyring {
  readonly scheme: 'Ed25519-WebCrypto' | 'HMAC-SHA256-WebCrypto';
  readonly publicKeyFingerprintHex: string;
  readonly sign: (data: Uint8Array) => Promise<Uint8Array>;
  readonly verify: (data: Uint8Array, signature: Uint8Array) => Promise<boolean>;
}

export interface ChainVerificationResult {
  readonly valid: boolean;
  readonly checkedCount: number;
  readonly brokenSequence: number | null;
  readonly errorKind: 'ok' | 'prev-link-break' | 'hash-mismatch' | 'signature-invalid';
}

export const GENESIS_HASH_HEX = '0000000000000000000000000000000000000000000000000000000000000000';

function bytesToHex(bytes: Uint8Array): string {
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

function hexToBytes(hex: string): Uint8Array {
  const clean = hex.length % 2 === 0 ? hex : `0${hex}`;
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(clean.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function toArrayBuffer(bytes: Uint8Array): ArrayBuffer {
  const copy = new Uint8Array(bytes.byteLength);
  copy.set(bytes);
  return copy.buffer;
}

export function canonicalEntryPayload(
  entry: Omit<SignedAuditEntry, 'entryHashHex' | 'signatureHex' | 'reverted' | 'why'>
): string {
  return JSON.stringify({
    sequence: entry.sequence,
    id: entry.id,
    timestampIso: entry.timestampIso,
    actor: entry.actor,
    trustOrigin: entry.trustOrigin,
    title: entry.title,
    outcome: entry.outcome,
    reversible: entry.reversible,
    prevHashHex: entry.prevHashHex,
  });
}

export async function sha256Hex(input: string): Promise<string> {
  const enc = new TextEncoder().encode(input);
  const digest = await globalThis.crypto.subtle.digest('SHA-256', toArrayBuffer(enc));
  return bytesToHex(new Uint8Array(digest));
}

export async function createAuditKeyring(): Promise<AuditKeyring> {
  const subtle = globalThis.crypto.subtle;
  try {
    const kp = (await subtle.generateKey({ name: 'Ed25519' }, true, ['sign', 'verify'])) as CryptoKeyPair;
    const rawPub = new Uint8Array(await subtle.exportKey('raw', kp.publicKey));
    const fpDigest = await subtle.digest('SHA-256', toArrayBuffer(rawPub));
    return {
      scheme: 'Ed25519-WebCrypto',
      publicKeyFingerprintHex: bytesToHex(new Uint8Array(fpDigest)).slice(0, 32),
      sign: async (data: Uint8Array) =>
        new Uint8Array(await subtle.sign({ name: 'Ed25519' }, kp.privateKey, toArrayBuffer(data))),
      verify: async (data: Uint8Array, signature: Uint8Array) =>
        subtle.verify({ name: 'Ed25519' }, kp.publicKey, toArrayBuffer(signature), toArrayBuffer(data)),
    };
  } catch {
    const hmacKey = await subtle.generateKey({ name: 'HMAC', hash: 'SHA-256' }, true, ['sign', 'verify']);
    const rawKey = new Uint8Array(await subtle.exportKey('raw', hmacKey));
    const fpDigest = await subtle.digest('SHA-256', toArrayBuffer(rawKey));
    return {
      scheme: 'HMAC-SHA256-WebCrypto',
      publicKeyFingerprintHex: bytesToHex(new Uint8Array(fpDigest)).slice(0, 32),
      sign: async (data: Uint8Array) =>
        new Uint8Array(await subtle.sign('HMAC', hmacKey, toArrayBuffer(data))),
      verify: async (data: Uint8Array, signature: Uint8Array) =>
        subtle.verify('HMAC', hmacKey, toArrayBuffer(signature), toArrayBuffer(data)),
    };
  }
}

export async function appendSignedAuditEntry(
  chain: readonly SignedAuditEntry[],
  keyring: AuditKeyring,
  draft: {
    id: string;
    timestampIso: string;
    actor: SignedAuditEntry['actor'];
    trustOrigin: SignedAuditEntry['trustOrigin'];
    title: string;
    outcome: SignedAuditEntry['outcome'];
    reversible: boolean;
    why: WhySheetData;
  }
): Promise<SignedAuditEntry[]> {
  const last = chain[chain.length - 1];
  const prevHashHex = last ? last.entryHashHex : GENESIS_HASH_HEX;
  const sequence = chain.length + 1;

  const base = {
    sequence,
    id: draft.id,
    timestampIso: draft.timestampIso,
    actor: draft.actor,
    trustOrigin: draft.trustOrigin,
    title: draft.title,
    outcome: draft.outcome,
    reversible: draft.reversible,
    prevHashHex,
    signatureScheme: keyring.scheme,
  } as const;

  const canonical = canonicalEntryPayload(base);
  const entryHashHex = await sha256Hex(`${prevHashHex}:${canonical}`);
  const sigBytes = await keyring.sign(new TextEncoder().encode(entryHashHex));

  const signedEntry: SignedAuditEntry = {
    ...base,
    reverted: false,
    entryHashHex,
    signatureHex: bytesToHex(sigBytes),
    why: draft.why,
  };

  return [...chain, signedEntry];
}

export async function verifyAuditChain(
  chain: readonly SignedAuditEntry[],
  keyring: AuditKeyring
): Promise<ChainVerificationResult> {
  let expectedPrev = GENESIS_HASH_HEX;

  for (const entry of chain) {
    if (entry.prevHashHex !== expectedPrev) {
      return {
        valid: false,
        checkedCount: entry.sequence,
        brokenSequence: entry.sequence,
        errorKind: 'prev-link-break',
      };
    }

    const canonical = canonicalEntryPayload(entry);
    const recomputedHash = await sha256Hex(`${entry.prevHashHex}:${canonical}`);
    if (recomputedHash !== entry.entryHashHex) {
      return {
        valid: false,
        checkedCount: entry.sequence,
        brokenSequence: entry.sequence,
        errorKind: 'hash-mismatch',
      };
    }

    const sigValid = await keyring.verify(
      new TextEncoder().encode(entry.entryHashHex),
      hexToBytes(entry.signatureHex)
    );
    if (!sigValid) {
      return {
        valid: false,
        checkedCount: entry.sequence,
        brokenSequence: entry.sequence,
        errorKind: 'signature-invalid',
      };
    }

    expectedPrev = entry.entryHashHex;
  }

  return {
    valid: true,
    checkedCount: chain.length,
    brokenSequence: null,
    errorKind: 'ok',
  };
}

/**
 * Simulates an attacker mutating a single field inside an already-sealed block
 * without holding the root signing seed or rewriting subsequent hashes.
 */
export function tamperAuditEntryTitle(
  chain: readonly SignedAuditEntry[],
  targetSequence: number,
  forgedTitle: string
): SignedAuditEntry[] {
  return chain.map((entry) =>
    entry.sequence === targetSequence ? { ...entry, title: forgedTitle } : entry
  );
}
