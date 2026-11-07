import { describe, expect, it } from 'vitest';
import {
  ALL_13_CANONICAL_SCOPES,
  ALL_5_ACTIONS,
  MAX_DELEGATION_DEPTH,
  PINNED_ROOT_PUBLIC_KEY,
  attenuateToken,
  attemptPrivilegeEscalationAttack,
  buildRootToken,
  extractEffectiveCapabilities,
  formatDatalogCode,
  type BiscuitBlock,
  type CapabilityAction,
  type CapabilityScope,
} from '@/engine/biscuitEngine';

describe('Feature 2: Biscuit v5 Capability Token Inspector & Attenuator', () => {
  // =========================================================================
  // TIER 1: FEATURE COVERAGE (>= 5 tests)
  // =========================================================================
  describe('Tier 1: Feature Coverage', () => {
    it('T1.1 - builds authoritative Block 0 root token with pinned authority and TTL check', () => {
      const scopes: CapabilityScope[] = ['Notification', 'Filesystem'];
      const actions: CapabilityAction[] = ['Read', 'Write'];
      const ttl = 3600;

      const token = buildRootToken(scopes, actions, ttl);

      expect(token).toHaveLength(1);
      const rootBlock = token[0]!;
      expect(rootBlock.index).toBe(0);
      expect(rootBlock.authority).toBe('RootSeed');
      expect(rootBlock.label).toContain('Root User Seed');
      expect(rootBlock.signatureHex).toMatch(/^sig_[0-9a-f]{8}0e25519$/);

      // Verify Datalog facts
      expect(rootBlock.facts).toContain('right("notification", "read");');
      expect(rootBlock.facts).toContain('right("notification", "write");');
      expect(rootBlock.facts).toContain('right("filesystem", "read");');
      expect(rootBlock.facts).toContain('right("filesystem", "write");');

      // Verify TTL check
      expect(rootBlock.checks[0]).toMatch(/check if time\(\$time\), \$time <= "[^"]+"; \/\/ TTL: 3600s/);
    });

    it('T1.2 - performs strictly monotonic offline attenuation to a subset of scopes and actions', () => {
      const root = buildRootToken(['Notification', 'Filesystem', 'Camera'], ['Read', 'Write', 'Execute'], 3600);
      const childScopes: CapabilityScope[] = ['Notification'];
      const childActions: CapabilityAction[] = ['Read'];

      const result = attenuateToken(root, childScopes, childActions);

      expect(result.valid).toBe(true);
      expect(result.violationReason).toBeNull();
      expect(result.attenuationDepth).toBe(1);
      expect(result.grantedScopes).toEqual(['Notification']);
      expect(result.grantedActions).toEqual(['Read']);
      expect(result.datalogCode).toContain('allowed_scope("notification");');
      expect(result.datalogCode).toContain('allowed_action("read");');
    });

    it('T1.3 - indexes multi-block delegation chain with correct authorities', () => {
      const root = buildRootToken(['Notification', 'Audio'], ['Read', 'Write'], 3600);

      // First attenuation -> SubAgentAttenuation
      const step1 = attenuateToken(root, ['Notification', 'Audio'], ['Read', 'Write']);
      expect(step1.valid).toBe(true);
      expect(step1.attenuationDepth).toBe(1);

      // Construct 2-block chain
      const chain2: BiscuitBlock[] = [
        root[0]!,
        {
          index: 1,
          label: 'Block 1',
          authority: 'SubAgentAttenuation',
          facts: ['allowed_scope("notification");'],
          checks: [],
          signatureHex: 'sig_step1',
        },
      ];

      // Second attenuation -> ThirdPartyWorker
      const step2 = attenuateToken(chain2, ['Notification'], ['Read']);
      expect(step2.valid).toBe(true);
      expect(step2.attenuationDepth).toBe(2);
      expect(step2.grantedScopes).toEqual(['Notification']);
      expect(step2.grantedActions).toEqual(['Read']);
    });

    it('T1.4 - generates valid single-pass Datalog representation', () => {
      const root = buildRootToken(['Notification'], ['Read'], 600);
      expect(formatDatalogCode(root)).toContain('// --- Block 0: Root User Seed');
      const res = attenuateToken(root, ['Notification'], ['Read'], ['check if resource("alert_123");']);

      expect(res.valid).toBe(true);
      const datalog = res.datalogCode;
      expect(datalog).toContain('// --- Block 0: Root User Seed');
      expect(datalog).toContain('// --- Block 1: Delegated Attenuation');
      expect(datalog).toContain('check if resource("alert_123");');
      expect(datalog).toContain('check if operation($scope, $action), allowed_scope($scope), allowed_action($action);');
    });

    it('T1.5 - accurately computes effective capabilities across multiple attenuation blocks', () => {
      const root = buildRootToken(
        ['Notification', 'Filesystem', 'Camera', 'Audio'],
        ['Read', 'Write', 'Execute', 'Delete'],
        1800
      );

      // Intersecting scopes
      const initialEffective = extractEffectiveCapabilities(root);
      expect(initialEffective.scopes).toHaveLength(4);
      expect(initialEffective.actions).toHaveLength(4);

      // Simulated block chain with two successive reductions
      const block1: BiscuitBlock = {
        index: 1,
        label: 'Block 1',
        authority: 'SubAgentAttenuation',
        facts: ['allowed_scope("notification");', 'allowed_scope("camera");', 'allowed_action("read");', 'allowed_action("write");'],
        checks: [],
        signatureHex: 'sig1',
      };
      const block2: BiscuitBlock = {
        index: 2,
        label: 'Block 2',
        authority: 'ThirdPartyWorker',
        facts: ['allowed_scope("notification");', 'allowed_action("read");'],
        checks: [],
        signatureHex: 'sig2',
      };

      const chain = [...root, block1, block2];
      const finalEffective = extractEffectiveCapabilities(chain);
      expect(finalEffective.scopes).toEqual(['Notification']);
      expect(finalEffective.actions).toEqual(['Read']);
    });

    it('T1.6 - enforces all 13 canonical scopes and 5 actions inventory', () => {
      expect(ALL_13_CANONICAL_SCOPES).toHaveLength(13);
      expect(ALL_13_CANONICAL_SCOPES).toContain('Notification');
      expect(ALL_13_CANONICAL_SCOPES).toContain('Wallet');
      expect(ALL_13_CANONICAL_SCOPES).toContain('FullSystem');
      expect(ALL_13_CANONICAL_SCOPES).toContain('CognitiveInference');
      expect(ALL_5_ACTIONS).toEqual(['Read', 'Write', 'Execute', 'Delete', 'Admin']);
      expect(MAX_DELEGATION_DEPTH).toBe(4);
      expect(PINNED_ROOT_PUBLIC_KEY).toMatch(/^ed25519:[0-9a-f]{64}$/);
    });
  });

  // =========================================================================
  // TIER 2: BOUNDARY & CORNER CASES (>= 5 tests)
  // =========================================================================
  describe('Tier 2: Boundary & Corner Cases', () => {
    it('T2.1 - rejects scope widening: child requesting Wallet when parent only holds Filesystem', () => {
      const parent = buildRootToken(['Filesystem'], ['Read'], 3600);
      const attackScopes: CapabilityScope[] = ['Filesystem', 'Wallet'];

      const result = attenuateToken(parent, attackScopes, ['Read']);

      expect(result.valid).toBe(false);
      expect(result.violationReason).toContain('RULE-04: Monotonic Attenuation Violation');
      expect(result.violationReason).toContain('Attempted to widen scope with "Wallet"');
      expect(result.grantedScopes).not.toContain('Wallet');
    });

    it('T2.2 - rejects action widening: child requesting Admin when parent only holds Read', () => {
      const parent = buildRootToken(['Notification'], ['Read'], 3600);
      const attackActions: CapabilityAction[] = ['Read', 'Admin'];

      const result = attenuateToken(parent, ['Notification'], attackActions);

      expect(result.valid).toBe(false);
      expect(result.violationReason).toContain('RULE-04: Monotonic Attenuation Violation');
      expect(result.violationReason).toContain('Attempted to add action "Admin"');
      expect(result.grantedActions).not.toContain('Admin');
    });

    it('T2.3 - rejects attenuation of empty token chain', () => {
      const result = attenuateToken([], ['Notification'], ['Read']);

      expect(result.valid).toBe(false);
      expect(result.violationReason).toContain('RULE-00: Missing root block');
      expect(result.attenuationDepth).toBe(0);
    });

    it('T2.4 - permits exact delegation depth boundary of 4 (total 5 blocks)', () => {
      const root = buildRootToken(['Notification'], ['Read'], 3600);
      let currentChain = root;

      // Add 4 successive attenuations (depth 1, 2, 3, 4)
      for (let i = 1; i <= 4; i++) {
        const att = attenuateToken(currentChain, ['Notification'], ['Read']);
        expect(att.valid).toBe(true);
        expect(att.attenuationDepth).toBe(i);
        currentChain = [
          ...currentChain,
          {
            index: i,
            label: `Block ${i}`,
            authority: i === 1 ? 'SubAgentAttenuation' : 'ThirdPartyWorker',
            facts: ['allowed_scope("notification");', 'allowed_action("read");'],
            checks: [],
            signatureHex: `sig_${i}`,
          },
        ];
      }

      expect(currentChain).toHaveLength(5); // 1 root + 4 attenuations
    });

    it('T2.5 - rejects 5th attenuation attempt (depth overflow D = 5 > D_max = 4)', () => {
      const root = buildRootToken(['Notification'], ['Read'], 3600);
      const chain5Blocks: BiscuitBlock[] = [
        root[0]!,
        { index: 1, label: 'B1', authority: 'SubAgentAttenuation', facts: ['allowed_scope("notification");'], checks: [], signatureHex: 's1' },
        { index: 2, label: 'B2', authority: 'ThirdPartyWorker', facts: ['allowed_scope("notification");'], checks: [], signatureHex: 's2' },
        { index: 3, label: 'B3', authority: 'ThirdPartyWorker', facts: ['allowed_scope("notification");'], checks: [], signatureHex: 's3' },
        { index: 4, label: 'B4', authority: 'ThirdPartyWorker', facts: ['allowed_scope("notification");'], checks: [], signatureHex: 's4' },
      ];

      // Current depth is 4 (5 blocks total). Attempting 5th attenuation:
      const overflowResult = attenuateToken(chain5Blocks, ['Notification'], ['Read']);

      expect(overflowResult.valid).toBe(false);
      expect(overflowResult.violationReason).toContain('RULE-05: Attenuation depth limit exceeded');
      expect(overflowResult.violationReason).toContain('D_max = 4');
      expect(overflowResult.attenuationDepth).toBe(4);
    });

    it('T2.6 - detects and repels widen_scope privilege escalation attack', () => {
      const token = buildRootToken(['Notification'], ['Read'], 1800);
      const attackResult = attemptPrivilegeEscalationAttack(token, 'widen_scope');

      expect(attackResult.valid).toBe(false);
      expect(attackResult.violationReason).toContain('RULE-04: Monotonic Attenuation Violation');
      expect(attackResult.violationReason).toContain('ungranted scope "Wallet"');
      expect(attackResult.grantedScopes).not.toContain('Wallet');
    });

    it('T2.7 - detects and repels add_action wildcard escalation attack', () => {
      const token = buildRootToken(['Notification'], ['Read'], 1800);
      const attackResult = attemptPrivilegeEscalationAttack(token, 'add_action');

      expect(attackResult.valid).toBe(false);
      expect(attackResult.violationReason).toContain('RULE-04: Monotonic Attenuation Violation');
      expect(attackResult.violationReason).toContain('elevate to "Admin"');
      expect(attackResult.grantedActions).not.toContain('Admin');
    });

    it('T2.8 - detects caveat stripping attack and flags signature rupture', () => {
      const token = buildRootToken(['Notification'], ['Read'], 1800);
      const attackResult = attemptPrivilegeEscalationAttack(token, 'remove_caveat');

      expect(attackResult.valid).toBe(false);
      expect(attackResult.violationReason).toContain('RULE-04: Signature Verification Failed');
      expect(attackResult.violationReason).toContain('Parent block checks were stripped');
    });

    it('T2.9 - rejects forged root block signed by rogue keypair', () => {
      const token = buildRootToken(['Notification'], ['Read'], 1800);
      const attackResult = attemptPrivilegeEscalationAttack(token, 'forge_root');

      expect(attackResult.valid).toBe(false);
      expect(attackResult.violationReason).toContain('RULE-03: Root Public Key Mismatch');
      expect(attackResult.violationReason).toContain('Token Block 0 was signed by untrusted key');
    });
  });

  // =========================================================================
  // TIER 3: CROSS-FEATURE PAIRWISE COMBINATIONS (>= 2 tests)
  // =========================================================================
  describe('Tier 3: Cross-Feature Combinations', () => {
    it('T3.1 - Biscuit attenuation combined with custom Datalog checks and scope bounds', () => {
      const root = buildRootToken(['Notification', 'Filesystem'], ['Read', 'Write'], 3600);

      // Attenuate to Notification only with custom resource check
      const customChecks = [
        'check if resource("topic://kernel/alerts");',
        'check if client_ip("127.0.0.1");',
      ];
      const child = attenuateToken(root, ['Notification'], ['Read'], customChecks);

      expect(child.valid).toBe(true);
      expect(child.grantedScopes).toEqual(['Notification']);
      expect(child.grantedActions).toEqual(['Read']);
      expect(child.datalogCode).toContain('check if resource("topic://kernel/alerts");');
      expect(child.datalogCode).toContain('check if client_ip("127.0.0.1");');
    });

    it('T3.2 - Multi-step sub-agent delegation preserves strict monotonically decreasing capability set', () => {
      // Step 0: User Root grants 3 scopes, 3 actions
      const root = buildRootToken(['Notification', 'ModelInference', 'Filesystem'], ['Read', 'Write', 'Execute'], 7200);

      // Step 1: Sub-agent restricts to 2 scopes, 2 actions
      const step1Result = attenuateToken(root, ['Notification', 'ModelInference'], ['Read', 'Execute']);
      expect(step1Result.valid).toBe(true);
      const block1: BiscuitBlock = {
        index: 1,
        label: 'Block 1: Orchestrator Sub-Agent',
        authority: 'SubAgentAttenuation',
        facts: ['allowed_scope("notification");', 'allowed_scope("modelinference");', 'allowed_action("read");', 'allowed_action("execute");'],
        checks: [],
        signatureHex: 'sig_subagent',
      };

      // Step 2: Worker restricts to 1 scope, 1 action
      const chain = [...root, block1];
      const step2Result = attenuateToken(chain, ['Notification'], ['Read']);
      expect(step2Result.valid).toBe(true);
      expect(step2Result.grantedScopes).toEqual(['Notification']);
      expect(step2Result.grantedActions).toEqual(['Read']);

      // Attempting to jump back to Filesystem in Step 3 fails
      const block2: BiscuitBlock = {
        index: 2,
        label: 'Block 2: Worker',
        authority: 'ThirdPartyWorker',
        facts: ['allowed_scope("notification");', 'allowed_action("read");'],
        checks: [],
        signatureHex: 'sig_worker',
      };
      const chain2 = [...chain, block2];
      const step3Illegal = attenuateToken(chain2, ['Filesystem'], ['Read']);
      expect(step3Illegal.valid).toBe(false);
      expect(step3Illegal.violationReason).toContain('Monotonic Attenuation Violation');
    });
  });

  // =========================================================================
  // TIER 4: REAL-WORLD APPLICATION SCENARIOS
  // =========================================================================
  describe('Tier 4: Real-World Scenarios', () => {
    it('T4.1 - Scenario 3: Sandboxed Sub-Agent Attenuation & Delegation', () => {
      // Background: A user runs an autonomous code-review agent.
      // 1. Root token is minted with scoped rights (Notification, ModelInference, Filesystem).
      const rootToken = buildRootToken(
        ['Notification', 'ModelInference', 'Filesystem'],
        ['Read', 'Write', 'Execute'],
        3600
      );
      expect(rootToken[0]?.authority).toBe('RootSeed');

      // 2. Main agent attenuates token for a sandboxed linter worker:
      // It restricts scopes to 'Filesystem' only, and actions to 'Read' only, with a path check.
      const linterAtt = attenuateToken(
        rootToken,
        ['Filesystem'],
        ['Read'],
        ['check if path_prefix("/home/antigravity/hakille/src");']
      );

      expect(linterAtt.valid).toBe(true);
      expect(linterAtt.grantedScopes).toEqual(['Filesystem']);
      expect(linterAtt.grantedActions).toEqual(['Read']);
      expect(linterAtt.attenuationDepth).toBe(1);
      expect(linterAtt.datalogCode).toContain('check if path_prefix("/home/antigravity/hakille/src");');

      // 3. The worker is completely barred from Network or Wallet
      const extract = extractEffectiveCapabilities([
        rootToken[0]!,
        {
          index: 1,
          label: 'Linter Block',
          authority: 'ThirdPartyWorker',
          facts: ['allowed_scope("filesystem");', 'allowed_action("read");'],
          checks: ['check if path_prefix("/home/antigravity/hakille/src");'],
          signatureHex: 'sig_linter',
        },
      ]);
      expect(extract.scopes).toEqual(['Filesystem']);
      expect(extract.actions).toEqual(['Read']);
    });

    it('T4.2 - Scenario 4: Rogue Agent Privilege Escalation Attack', () => {
      // An untrusted third-party plugin receives an attenuated token with Notification Read.
      const root = buildRootToken(['Notification'], ['Read'], 1800);
      const subAgentToken: BiscuitBlock[] = [
        root[0]!,
        {
          index: 1,
          label: 'Block 1: Plugin Token',
          authority: 'ThirdPartyWorker',
          facts: ['allowed_scope("notification");', 'allowed_action("read");'],
          checks: ['check if time($time), $time <= "2026-11-07T12:00:00Z";'],
          signatureHex: 'sig_plugin_block_1',
        },
      ];

      // Attack Vector A: Plugin attempts to widen its scope to Wallet to sign a financial transaction.
      const attackA = attemptPrivilegeEscalationAttack(subAgentToken, 'widen_scope');
      expect(attackA.valid).toBe(false);
      expect(attackA.violationReason).toContain('RULE-04');
      expect(attackA.grantedScopes).not.toContain('Wallet');

      // Attack Vector B: Plugin attempts to strip the TTL check to remain active forever.
      const attackB = attemptPrivilegeEscalationAttack(subAgentToken, 'remove_caveat');
      expect(attackB.valid).toBe(false);
      expect(attackB.violationReason).toContain('Signature Verification Failed');

      // Attack Vector C: Plugin attempts to mint its own root authority block.
      const attackC = attemptPrivilegeEscalationAttack(subAgentToken, 'forge_root');
      expect(attackC.valid).toBe(false);
      expect(attackC.violationReason).toContain('Root Public Key Mismatch');
    });
  });
});
