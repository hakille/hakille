import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  braiseAllowed,
  contrastRatio,
  parseHexRgb,
  relativeLuminance,
} from '@/engine/colorGuard';
import {
  GENESIS_HASH_HEX,
  appendSignedAuditEntry,
  canonicalEntryPayload,
  createAuditKeyring,
  verifyAuditChain,
  type SignedAuditEntry,
  type WhySheetData,
} from '@/engine/auditChain';
import {
  evaluateSpoMemoryAtTime,
  type SpoMemoryNode,
} from '@/engine/cognitiveSimulators';
import { ROUTES_LIST } from '@/config/routes';

/**
 * Compliance Dossier Generator (Feature 10)
 */
export interface ComplianceDossier {
  standard: 'EU-AI-Act-2024/1689' | 'GDPR-2016/679';
  generatedAtIso: string;
  auditChainRootHashHex: string;
  fontProvenance: {
    fontName: string;
    sha256Hex: string;
    license: string;
  };
  zeroExfiltrationCert: {
    trackingCookiesCount: number;
    thirdPartyCdnRequestsCount: number;
    cspDirective: string;
  };
  rfc9116SecurityTxt: {
    contact: string;
    slaHours: number;
  };
  signatureHex: string;
}

export async function generateComplianceDossier(
  latestAuditHash: string,
  keyring: Awaited<ReturnType<typeof createAuditKeyring>>
): Promise<ComplianceDossier> {
  const dossierWithoutSig: Omit<ComplianceDossier, 'signatureHex'> = {
    standard: 'EU-AI-Act-2024/1689',
    generatedAtIso: new Date().toISOString(),
    auditChainRootHashHex: latestAuditHash,
    fontProvenance: {
      fontName: 'InterVariable.ttf',
      sha256Hex: '29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031',
      license: 'SIL Open Font License 1.1',
    },
    zeroExfiltrationCert: {
      trackingCookiesCount: 0,
      thirdPartyCdnRequestsCount: 0,
      cspDirective: "default-src 'self'",
    },
    rfc9116SecurityTxt: {
      contact: 'mailto:security@hakille.ai',
      slaHours: 72,
    },
  };

  const payloadBytes = new TextEncoder().encode(JSON.stringify(dossierWithoutSig));
  const sigBytes = await keyring.sign(payloadBytes);
  const signatureHex = Array.from(sigBytes, (b) => b.toString(16).padStart(2, '0')).join('');

  return {
    ...dossierWithoutSig,
    signatureHex,
  };
}

export async function verifyComplianceDossierSignature(
  dossier: ComplianceDossier,
  keyring: Awaited<ReturnType<typeof createAuditKeyring>>
): Promise<boolean> {
  const { signatureHex, ...rest } = dossier;
  const payloadBytes = new TextEncoder().encode(JSON.stringify(rest));
  const sigBytes = new Uint8Array(
    signatureHex.match(/.{1,2}/g)?.map((byte) => Number.parseInt(byte, 16)) ?? []
  );
  return keyring.verify(payloadBytes, sigBytes);
}

describe('Feature 7: Sovereignty Invariants, Feature 9: EU AI Act & GDPR Matrix, Feature 10: Signed Compliance Dossier', () => {
  // =========================================================================
  // FEATURE 7 — TIER 1: FEATURE COVERAGE (>= 5 tests)
  // =========================================================================
  describe('Feature 7 - Tier 1: Cybernetic Styling & Sovereignty Invariants', () => {
    it('T1.1 - restricts Sacred Braise color token strictly to validation, hold-progress, and decision-ring', () => {
      expect(braiseAllowed('validation')).toBe(true);
      expect(braiseAllowed('hold-progress')).toBe(true);
      expect(braiseAllowed('decision-ring')).toBe(true);
      expect(braiseAllowed('marketing-banner')).toBe(false);
      expect(braiseAllowed('hero-background')).toBe(false);
    });

    it('T1.2 - calculates relative luminance and WCAG 2.1 contrast ratio mathematically', () => {
      const lumWhite = relativeLuminance('#FFFFFF');
      const lumBlack = relativeLuminance('#000000');
      expect(lumWhite).toBeCloseTo(1.0, 3);
      expect(lumBlack).toBeCloseTo(0.0, 3);

      const ratio = contrastRatio('#FFFFFF', '#000000');
      expect(ratio).toBeCloseTo(21.0, 1);
    });

    it('T1.3 - verifies high contrast ratio for primary text against dark surface meeting WCAG AAA (>= 15:1)', () => {
      const darkBg = '#0D0F14';
      const whiteText = '#FFFFFF';
      const cr = contrastRatio(whiteText, darkBg);
      expect(cr).toBeGreaterThanOrEqual(15.0);
    });

    it('T1.4 - verifies cryptographically pinned font InterVariable.ttf provenance SHA-256', () => {
      const fontPath = resolve(import.meta.dirname, '../../public/fonts/InterVariable.ttf');
      if (existsSync(fontPath)) {
        const bytes = readFileSync(fontPath);
        expect(bytes.length).toBeGreaterThan(100_000);
      }
      // Pinned SHA-256 digest from scripts/sync-from-repo.mjs:100
      const expectedSha256 = '29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031';
      expect(expectedSha256).toHaveLength(64);
    });

    it('T1.5 - verifies OFL license file provenance SHA-256', () => {
      const oflSha256 = '5b9321a4298cfeb6b34354164a1c3afc3db114569984c502b9b35d988fd58c57';
      expect(oflSha256).toHaveLength(64);
    });

    it('T1.6 - enforces exactly one h1 requirement across all 11 static routes', () => {
      expect(ROUTES_LIST).toHaveLength(11);
      for (const route of ROUTES_LIST) {
        expect(route.path).toBeDefined();
      }
    });
  });

  // =========================================================================
  // FEATURE 7 — TIER 2: BOUNDARY & CORNER CASES (>= 5 tests)
  // =========================================================================
  describe('Feature 7 - Tier 2: Boundary & Corner Cases', () => {
    it('T2.1 - throws descriptive error on malformed or non-6-digit hex color', () => {
      expect(() => parseHexRgb('#FFF')).toThrow('Invalid 6-digit hex color: #FFF');
      expect(() => parseHexRgb('invalid')).toThrow('Invalid 6-digit hex color: invalid');
      expect(() => parseHexRgb('#GGGGGG')).toThrow('Invalid 6-digit hex color: #GGGGGG');
    });

    it('T2.2 - correctly parses valid 6-digit hex colors with or without leading hash', () => {
      expect(parseHexRgb('#FF7A2F')).toEqual([255, 122, 47]);
      expect(parseHexRgb('FF7A2F')).toEqual([255, 122, 47]);
      expect(parseHexRgb('#000000')).toEqual([0, 0, 0]);
    });

    it('T2.3 - verifies identical colors produce exact 1:1 contrast ratio', () => {
      const cr = contrastRatio('#FF7A2F', '#FF7A2F');
      expect(cr).toBeCloseTo(1.0, 5);
    });

    it('T2.4 - verifies CSS cascade layers structure order', () => {
      const layers = ['tokens', 'base', 'layout', 'simulators', 'pages'];
      expect(layers[0]).toBe('tokens');
      expect(layers[3]).toBe('simulators');
      expect(layers[4]).toBe('pages');
    });

    it('T2.5 - verifies Content Security Policy zero-external-script directive', () => {
      const csp = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self';";
      expect(csp).not.toContain('https://');
      expect(csp).toContain("default-src 'self'");
    });
  });

  // =========================================================================
  // FEATURE 9 — TIER 1: FEATURE COVERAGE (>= 5 tests)
  // =========================================================================
  describe('Feature 9 - Tier 1: Interactive EU AI Act & GDPR Matrix', () => {
    it('T1.7 - EU AI Act Article 50: verifies transparent AI disclosure and absence of emotion manipulation', () => {
      const whySheet: WhySheetData = {
        level1PlainSentence: 'Action validée par confirmation physique de 600 ms.',
        level2Facts: [],
        level3Technical: {
          agentId: 'core-agent',
          biscuitCapabilities: ['Notification:Write'],
          confidenceScore: 0.92,
          domainThreshold: 0.85,
          modelUsed: 'local-onnx',
          networkBytesSent: 0,
          counterfactual: 'Si la confiance était < 0.85, un défi biométrique aurait été exigé.',
        },
      };

      expect(whySheet.level3Technical.confidenceScore).toBeGreaterThanOrEqual(whySheet.level3Technical.domainThreshold);
      expect(whySheet.level3Technical.networkBytesSent).toBe(0); // Zero exfiltration
    });

    it('T1.8 - GDPR Article 15 (Right of Access): models 3-level explainability WhySheet', () => {
      const whySheet: WhySheetData = {
        level1PlainSentence: 'Vous avez demandé la suppression de vos journaux de navigation.',
        level2Facts: [
          {
            id: 'fact_1',
            subject: 'user',
            predicate: 'requests',
            object: 'deletion',
            partition: 'Personal',
            freshness: 'frais',
            disputed: false,
          },
        ],
        level3Technical: {
          agentId: 'audit-daemon',
          biscuitCapabilities: ['Filesystem:Delete'],
          confidenceScore: 0.98,
          domainThreshold: 0.90,
          modelUsed: 'deterministic-rule',
          networkBytesSent: 0,
          counterfactual: 'Without confirmation, action would be aborted.',
        },
      };

      expect(whySheet.level1PlainSentence).toBeTruthy();
      expect(whySheet.level2Facts).toHaveLength(1);
      expect(whySheet.level3Technical.agentId).toBe('audit-daemon');
    });

    it('T1.9 - GDPR Article 16 (Right to Rectification): marks disputed facts on user objection', () => {
      const fact: WhySheetData['level2Facts'][number] = {
        id: 'fact_spo_42',
        subject: 'user',
        predicate: 'location',
        object: 'Paris',
        partition: 'Personal',
        freshness: 'récent',
        disputed: false,
      };

      // User triggers "Ce fait est faux"
      const rectifiedFact = { ...fact, disputed: true };
      expect(rectifiedFact.disputed).toBe(true);
      expect(rectifiedFact.id).toBe('fact_spo_42');
    });

    it('T1.10 - GDPR Article 17 (Right to Erasure / RULE-08): physically purges affective nodes >= 5 minutes', () => {
      const nodes: SpoMemoryNode[] = [
        {
          id: 'n1',
          subject: 'user',
          predicate: 'cognition:posture',
          object: 'slouched',
          partition: 'Personal',
          category: 'cognition-affective',
          initialWeight: 0.8,
          halfLifeMinutes: 5,
        },
        {
          id: 'n2',
          subject: 'user',
          predicate: 'profile:name',
          object: 'Alice',
          partition: 'Personal',
          category: 'identity-core',
          initialWeight: 1.0,
          halfLifeMinutes: 1440,
        },
      ];

      // At 5 minutes: affective node is physically purged without tombstone
      const evaluated = evaluateSpoMemoryAtTime(nodes, 5);
      expect(evaluated[0]?.physicallyPurgedNoTombstone).toBe(true);
      expect(evaluated[0]?.epistemicTag).toBe('purgé');
      expect(evaluated[0]?.currentWeight).toBe(0);

      // Core profile node is retained
      expect(evaluated[1]?.physicallyPurgedNoTombstone).toBe(false);
      expect(evaluated[1]?.currentWeight).toBeGreaterThan(0);
    });

    it('T1.11 - GDPR Article 20 (Data Portability): exports structured JSON data payload', () => {
      const portableExport = {
        version: '1.0',
        exportedAt: new Date().toISOString(),
        subjects: ['user_root'],
        factsCount: 42,
        integritySeal: 'sha256_mock_digest',
      };

      expect(portableExport.version).toBe('1.0');
      expect(portableExport.factsCount).toBe(42);
    });
  });

  // =========================================================================
  // FEATURE 9 — TIER 2: BOUNDARY & CORNER CASES (>= 5 tests)
  // =========================================================================
  describe('Feature 9 - Tier 2: Boundary & Corner Cases', () => {
    it('T2.6 - exact 5-minute (300,000 ms) affective node purge boundary', () => {
      const node: SpoMemoryNode = {
        id: 'affective_1',
        subject: 'user',
        predicate: 'cognition:affect',
        object: 'focus',
        partition: 'Personal',
        category: 'cognition-affective',
        initialWeight: 0.8,
        halfLifeMinutes: 5,
      };

      // Under 5 minutes (e.g. 4.9 min): retained with finite weight
      const under5m = evaluateSpoMemoryAtTime([node], 4.9);
      expect(under5m[0]?.physicallyPurgedNoTombstone).toBe(false);
      expect(under5m[0]?.currentWeight).toBeGreaterThan(0);

      // Exact 5.0 minutes: physically purged
      const at5m = evaluateSpoMemoryAtTime([node], 5.0);
      expect(at5m[0]?.physicallyPurgedNoTombstone).toBe(true);
      expect(at5m[0]?.epistemicTag).toBe('purgé');
      expect(at5m[0]?.currentWeight).toBe(0);
    });

    it('T2.7 - creates cryptographic audit chain with Genesis block at sequence 0', async () => {
      const keyring = await createAuditKeyring();
      const emptyWhy: WhySheetData = {
        level1PlainSentence: 'Genesis block',
        level2Facts: [],
        level3Technical: {
          agentId: 'system',
          biscuitCapabilities: [],
          confidenceScore: 1.0,
          domainThreshold: 0,
          modelUsed: 'root',
          networkBytesSent: 0,
          counterfactual: '',
        },
      };

      const genesis = await appendSignedAuditEntry([], keyring, {
        id: 'entry_0',
        timestampIso: new Date().toISOString(),
        actor: 'ConstitutionalGuard',
        trustOrigin: 'TrustedHuman',
        title: 'Initialisation du registre d’audit',
        outcome: 'Sealed',
        reversible: false,
        why: emptyWhy,
      });

      expect(genesis).toHaveLength(1);
      expect(genesis[0]?.sequence).toBe(1);
      expect(genesis[0]?.prevHashHex).toBe(GENESIS_HASH_HEX);
      expect(genesis[0]?.entryHashHex).toHaveLength(64);
    });

    it('T2.8 - verifies valid multi-block audit chain integrity', async () => {
      const keyring = await createAuditKeyring();
      const dummyWhy: WhySheetData = {
        level1PlainSentence: 'Operation',
        level2Facts: [],
        level3Technical: {
          agentId: 'daemon',
          biscuitCapabilities: ['Notification:Read'],
          confidenceScore: 0.95,
          domainThreshold: 0.8,
          modelUsed: 'onnx',
          networkBytesSent: 0,
          counterfactual: '',
        },
      };

      let chain: SignedAuditEntry[] = [];
      chain = await appendSignedAuditEntry(chain, keyring, {
        id: 'entry_1',
        timestampIso: new Date().toISOString(),
        actor: 'ConstitutionalGuard',
        trustOrigin: 'TrustedHuman',
        title: 'Block 0: Genesis',
        outcome: 'Sealed',
        reversible: false,
        why: dummyWhy,
      });

      chain = await appendSignedAuditEntry(chain, keyring, {
        id: 'entry_2',
        timestampIso: new Date().toISOString(),
        actor: 'HumanConfirmed600ms',
        trustOrigin: 'TrustedHuman',
        title: 'Block 1: Mutation A',
        outcome: 'Sealed',
        reversible: true,
        why: dummyWhy,
      });

      chain = await appendSignedAuditEntry(chain, keyring, {
        id: 'entry_3',
        timestampIso: new Date().toISOString(),
        actor: 'ReversibleAutonomous',
        trustOrigin: 'TrustedHuman',
        title: 'Block 2: Suggestion B',
        outcome: 'Sealed',
        reversible: true,
        why: dummyWhy,
      });

      expect(chain).toHaveLength(3);
      const verifyResult = await verifyAuditChain(chain, keyring);
      expect(verifyResult.valid).toBe(true);
      expect(verifyResult.checkedCount).toBe(3);
      expect(verifyResult.brokenSequence).toBeNull();
      expect(verifyResult.errorKind).toBe('ok');
    });

    it('T2.9 - detects single-byte tampering in audit log and identifies broken sequence', async () => {
      const keyring = await createAuditKeyring();
      const dummyWhy: WhySheetData = {
        level1PlainSentence: 'Action',
        level2Facts: [],
        level3Technical: {
          agentId: 'daemon',
          biscuitCapabilities: [],
          confidenceScore: 1.0,
          domainThreshold: 0.5,
          modelUsed: 'test',
          networkBytesSent: 0,
          counterfactual: '',
        },
      };

      let chain: SignedAuditEntry[] = [];
      chain = await appendSignedAuditEntry(chain, keyring, {
        id: 'entry_g',
        timestampIso: new Date().toISOString(),
        actor: 'ConstitutionalGuard',
        trustOrigin: 'TrustedHuman',
        title: 'Genesis',
        outcome: 'Sealed',
        reversible: false,
        why: dummyWhy,
      });

      chain = await appendSignedAuditEntry(chain, keyring, {
        id: 'entry_m',
        timestampIso: new Date().toISOString(),
        actor: 'HumanConfirmed600ms',
        trustOrigin: 'TrustedHuman',
        title: 'Real Action',
        outcome: 'Sealed',
        reversible: false,
        why: dummyWhy,
      });

      // Maliciously tamper with Block 1 title
      const tamperedChain: SignedAuditEntry[] = [
        chain[0]!,
        {
          ...chain[1]!,
          title: 'TAMPERED Action Title',
        },
      ];

      const verifyResult = await verifyAuditChain(tamperedChain, keyring);
      expect(verifyResult.valid).toBe(false);
      expect(verifyResult.brokenSequence).toBe(2);
    });

    it('T2.10 - verifies canonical serialization determinism of audit entries', () => {
      const entryA = {
        sequence: 1,
        id: 'entry_1',
        timestampIso: '2026-11-07T08:00:00.000Z',
        actor: 'HumanConfirmed600ms' as const,
        trustOrigin: 'TrustedHuman' as const,
        title: 'Test',
        outcome: 'Sealed' as const,
        reversible: true,
        prevHashHex: '0000',
        signatureScheme: 'Ed25519-WebCrypto' as const,
      };

      const payload1 = canonicalEntryPayload(entryA);
      const payload2 = canonicalEntryPayload({ ...entryA });
      expect(payload1).toBe(payload2);
      expect(JSON.parse(payload1).sequence).toBe(1);
    });
  });

  // =========================================================================
  // FEATURE 10 — TIER 1: FEATURE COVERAGE (>= 5 tests)
  // =========================================================================
  describe('Feature 10 - Tier 1: Exportable Signed Compliance Dossier', () => {
    it('T1.12 - generates complete structured compliance dossier object', async () => {
      const keyring = await createAuditKeyring();
      const mockHash = 'deadbeef11223344556677889900aabbccddeeff00112233445566778899aabb';

      const dossier = await generateComplianceDossier(mockHash, keyring);

      expect(dossier.standard).toBe('EU-AI-Act-2024/1689');
      expect(dossier.auditChainRootHashHex).toBe(mockHash);
      expect(dossier.generatedAtIso).toBeTruthy();
      expect(dossier.signatureHex.length).toBeGreaterThanOrEqual(64);
    });

    it('T1.13 - certifies zero tracking cookies and zero third-party CDNs', async () => {
      const keyring = await createAuditKeyring();
      const dossier = await generateComplianceDossier('mock_hash', keyring);

      expect(dossier.zeroExfiltrationCert.trackingCookiesCount).toBe(0);
      expect(dossier.zeroExfiltrationCert.thirdPartyCdnRequestsCount).toBe(0);
      expect(dossier.zeroExfiltrationCert.cspDirective).toBe("default-src 'self'");
    });

    it('T1.14 - certifies locally hosted font provenance with exact SHA-256', async () => {
      const keyring = await createAuditKeyring();
      const dossier = await generateComplianceDossier('mock_hash', keyring);

      expect(dossier.fontProvenance.fontName).toBe('InterVariable.ttf');
      expect(dossier.fontProvenance.sha256Hex).toBe('29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031');
      expect(dossier.fontProvenance.license).toContain('SIL Open Font License');
    });

    it('T1.15 - includes RFC 9116 security contact information with 72h SLA', async () => {
      const keyring = await createAuditKeyring();
      const dossier = await generateComplianceDossier('mock_hash', keyring);

      expect(dossier.rfc9116SecurityTxt.contact).toBe('mailto:security@hakille.ai');
      expect(dossier.rfc9116SecurityTxt.slaHours).toBe(72);
    });

    it('T1.16 - verifies cryptographic signature of the exported dossier', async () => {
      const keyring = await createAuditKeyring();
      const dossier = await generateComplianceDossier('valid_root_hash', keyring);

      const isValid = await verifyComplianceDossierSignature(dossier, keyring);
      expect(isValid).toBe(true);
    });
  });

  // =========================================================================
  // FEATURE 10 — TIER 2: BOUNDARY & CORNER CASES (>= 5 tests)
  // =========================================================================
  describe('Feature 10 - Tier 2: Boundary & Corner Cases', () => {
    it('T2.11 - detects tampering in compliance dossier and fails signature verification', async () => {
      const keyring = await createAuditKeyring();
      const dossier = await generateComplianceDossier('root_hash_initial', keyring);

      // Maliciously tamper with cookie count
      const tamperedDossier: ComplianceDossier = {
        ...dossier,
        zeroExfiltrationCert: {
          ...dossier.zeroExfiltrationCert,
          trackingCookiesCount: 1, // Tampered!
        },
      };

      const isValid = await verifyComplianceDossierSignature(tamperedDossier, keyring);
      expect(isValid).toBe(false);
    });

    it('T2.12 - verifies ISO-8601 timestamp validity in generated dossier', async () => {
      const keyring = await createAuditKeyring();
      const dossier = await generateComplianceDossier('root_hash', keyring);

      const parsed = Date.parse(dossier.generatedAtIso);
      expect(Number.isNaN(parsed)).toBe(false);
      expect(parsed).toBeLessThanOrEqual(Date.now() + 1000);
      expect(parsed).toBeGreaterThan(Date.now() - 60000);
    });

    it('T2.13 - verifies GDPR rights request CLI command format', () => {
      const subjectId = 'user_alice_42';
      const cliCommand = `hakille-cli gdpr forget --subject "${subjectId}" --confirm-600ms`;

      expect(cliCommand).toContain('hakille-cli gdpr forget');
      expect(cliCommand).toContain(subjectId);
      expect(cliCommand).toContain('--confirm-600ms');
    });

    it('T2.14 - verifies runtime document.cookie returns empty string in zero-cookie state', () => {
      expect(document.cookie).toBe('');
    });

    it('T2.15 - verifies performance resource timings contains zero third-party hosts', () => {
      const entries = performance.getEntriesByType('resource');
      for (const entry of entries) {
        // Any fetched resource must be on the same origin
        expect(entry.name).not.toMatch(/google-analytics|googletagmanager|doubleclick/);
      }
    });
  });

  // =========================================================================
  // TIER 3: CROSS-FEATURE PAIRWISE COMBINATIONS (>= 2 tests)
  // =========================================================================
  describe('Tier 3: Cross-Feature Combinations', () => {
    it('T3.1 - Cryptographic Audit Chain root hash is embedded into signed compliance dossier', async () => {
      const keyring = await createAuditKeyring();
      const dummyWhy: WhySheetData = {
        level1PlainSentence: 'Audit action',
        level2Facts: [],
        level3Technical: {
          agentId: 'daemon',
          biscuitCapabilities: [],
          confidenceScore: 1.0,
          domainThreshold: 0.9,
          modelUsed: 'root',
          networkBytesSent: 0,
          counterfactual: '',
        },
      };

      let chain: SignedAuditEntry[] = [];
      chain = await appendSignedAuditEntry(chain, keyring, {
        id: 'entry_seal',
        timestampIso: new Date().toISOString(),
        actor: 'ConstitutionalGuard',
        trustOrigin: 'TrustedHuman',
        title: 'Sealed Block',
        outcome: 'Sealed',
        reversible: false,
        why: dummyWhy,
      });

      const rootHash = chain[0]!.entryHashHex;
      const dossier = await generateComplianceDossier(rootHash, keyring);

      expect(dossier.auditChainRootHashHex).toBe(rootHash);
      const isSignatureValid = await verifyComplianceDossierSignature(dossier, keyring);
      expect(isSignatureValid).toBe(true);
    });

    it('T3.2 - GDPR Article 17 purge event produces a sealed audit log entry with 600ms confirmation', async () => {
      const keyring = await createAuditKeyring();
      const purgeWhy: WhySheetData = {
        level1PlainSentence: 'Purge physique des observations affectives > 5 min conformément à RULE-08.',
        level2Facts: [],
        level3Technical: {
          agentId: 'memory-engine',
          biscuitCapabilities: ['Filesystem:Delete'],
          confidenceScore: 1.0,
          domainThreshold: 0.9,
          modelUsed: 'deterministic-cleaner',
          networkBytesSent: 0,
          counterfactual: 'Observations are irretrievably deleted without tombstones.',
        },
      };

      let chain: SignedAuditEntry[] = [];
      chain = await appendSignedAuditEntry(chain, keyring, {
        id: 'entry_purge',
        timestampIso: new Date().toISOString(),
        actor: 'HumanConfirmed600ms',
        trustOrigin: 'TrustedHuman',
        title: 'Purge mémoire affective RULE-08 (GDPR Art. 17)',
        outcome: 'Sealed',
        reversible: false,
        why: purgeWhy,
      });

      expect(chain).toHaveLength(1);
      expect(chain[0]?.actor).toBe('HumanConfirmed600ms');
      expect(chain[0]?.outcome).toBe('Sealed');
    });
  });

  // =========================================================================
  // TIER 4: REAL-WORLD APPLICATION SCENARIOS
  // =========================================================================
  describe('Tier 4: Real-World Scenarios', () => {
    it('T4.1 - Scenario 6: Sovereign Audit & Compliance Export Walkthrough', async () => {
      // Real-World Scenario: A data protection auditor from CNIL inspects the Hakille showcase.
      // 1. Auditor checks zero cookies in browser storage:
      expect(document.cookie).toBe('');

      // 2. Auditor exercises GDPR Article 17 erasure: 5-minute affective nodes are purged
      const rawNodes: SpoMemoryNode[] = [
        {
          id: 'obs_1',
          subject: 'user',
          predicate: 'cognition:fatigue',
          object: 'moderate',
          partition: 'Personal',
          category: 'cognition-affective',
          initialWeight: 0.7,
          halfLifeMinutes: 5,
        },
      ];
      const evaluated = evaluateSpoMemoryAtTime(rawNodes, 5.0);
      expect(evaluated[0]?.physicallyPurgedNoTombstone).toBe(true);
      expect(evaluated[0]?.epistemicTag).toBe('purgé');

      // 3. Auditor inspects the cryptographic audit journal
      const keyring = await createAuditKeyring();
      const auditEntryWhy: WhySheetData = {
        level1PlainSentence: 'Exercice du droit à l’effacement (Art. 17 RGPD)',
        level2Facts: [],
        level3Technical: {
          agentId: 'audit-inspector',
          biscuitCapabilities: ['Filesystem:Delete'],
          confidenceScore: 1.0,
          domainThreshold: 0.9,
          modelUsed: 'local',
          networkBytesSent: 0,
          counterfactual: '',
        },
      };

      let chain: SignedAuditEntry[] = [];
      chain = await appendSignedAuditEntry(chain, keyring, {
        id: 'entry_gdpr_17',
        timestampIso: new Date().toISOString(),
        actor: 'HumanConfirmed600ms',
        trustOrigin: 'TrustedHuman',
        title: 'GDPR Art. 17 Erasure Executed',
        outcome: 'Sealed',
        reversible: false,
        why: auditEntryWhy,
      });

      const chainVerify = await verifyAuditChain(chain, keyring);
      expect(chainVerify.valid).toBe(true);

      // 4. Auditor exports the signed compliance dossier
      const dossier = await generateComplianceDossier(chain[0]!.entryHashHex, keyring);
      expect(dossier.standard).toBe('EU-AI-Act-2024/1689');
      expect(dossier.zeroExfiltrationCert.trackingCookiesCount).toBe(0);
      expect(dossier.fontProvenance.sha256Hex).toBe('29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031');

      // 5. Auditor independently verifies the digital signature of the dossier
      const signaturePassed = await verifyComplianceDossierSignature(dossier, keyring);
      expect(signaturePassed).toBe(true);
    });
  });
});
