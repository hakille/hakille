import { TranslationSchema } from '../types';

export const ptLocale: TranslationSchema = {
  meta: {
    brandSignature: 'Hakille — inteligência soberana',
    preAlphaBadge: 'Pré-Alfa v0.1.0 · Apache-2.0 · 100% Rust',
    skipToContent: 'Ir diretamente para o conteúdo principal',
  },
  routes: {
    home: {
      navLabel: 'Manifesto',
      title: 'hakille — inteligência soberana · Sistema Operativo Cognitivo em Rust Puro',
      description:
        'O sistema que nada esquece e prova sempre o que fez. Grafo de memória SPO local, capacidades Biscuit, retenção física de 600 ms e registo de auditoria SHA-256 + Ed25519.',
    },
    architecture: {
      navLabel: 'Arquitetura',
      title: 'Arquitetura de 6 Camadas e 41 Crates Rust — hakille',
      description:
        'Exploração profunda das 6 camadas do Hakille: 122 639 linhas de Rust 2024, 0% Python, shell nativo GPU winit+wgpu+AccessKit, Wasmtime 48 e Cognitive Fabric.',
    },
    honestStatus: {
      navLabel: 'Estado Real v0.1',
      title: 'Matriz de Honestidade Pré-Alfa v0.1 — hakille',
      description:
        'Transparência radical entre o que corre realmente hoje (1 468 testes, Whisper CPU ×46, codec Mimi, Qwen GGUF) e os marcos das Fases 1–3.',
    },
    security: {
      navLabel: 'Segurança',
      title: 'Capacidades Biscuit, Isolamento Untrusted e Segurança — hakille',
      description:
        'Tokens Biscuit v5 de atenuação monótona, imunidade estrutural contra prompt injection, cofre Argon2id HAKKEY02 e divulgação RFC 9116 (72h).',
    },
    constitution: {
      navLabel: 'Constituição',
      title: 'Constituição Inviolável v1.0.0 — hakille',
      description:
        'Os 7 Artigos e 10 Regras verificáveis por máquina acima de qualquer modelo: limiares de confiança, proteção de menores e protocolo de emergência.',
    },
    install: {
      navLabel: 'Instalar',
      title: 'Dimensionador de RAM, Substratos OS e Compilação Rust — hakille',
      description:
        'Calcule a pegada exata de memória local do Hakille na sua máquina (2 GB a 16 GB+) e compile o daemon, o desktop winit+wgpu ou a ponte móvel UniFFI.',
    },
    privacy: {
      navLabel: 'Privacidade & RGPD',
      title: 'Política de Privacidade RGPD, ePrivacy e AI Act — hakille',
      description:
        'Dupla garantia soberana: zero cookies, zero rastreadores e fontes OFL locais em hakille.ai; memória local redb e purga física de 5 min no Hakille OS.',
    },
    legalNotice: {
      navLabel: 'Aviso Legal',
      title: 'Menções Legais (LCEN) e Governação Open-Source — hakille',
      description:
        'Identificação do projeto open-source Hakille, contactos oficiais, alojamento estático soberano e propriedade intelectual.',
    },
    conditions: {
      navLabel: 'Termos',
      title: 'Termos de Utilização e Aviso Pré-Alfa AS-IS — hakille',
      description:
        'Condições de utilização de hakille.ai e avisos essenciais sobre o uso NO ESTADO EM QUE SE ENCONTRA (AS IS) do protótipo pré-alfa v0.1.',
    },
    licenses: {
      navLabel: 'Licenças & Patentes',
      title: 'Licença Apache-2.0, Compromisso de Patentes Defensivas e SHA-256 — hakille',
      description:
        'Licenciamento Apache-2.0, compromisso público de patentes defensivas, auditoria cargo-deny sem copyleft e proveniência SHA-256 das fontes SIL OFL 1.1.',
    },
    accessibility: {
      navLabel: 'Acessibilidade',
      title: 'Acessibilidade WCAG AAA/AA, Teclado e Escritas Mundiais — hakille',
      description:
        'Contraste AAA 17:1, alternativa de teclado em 2 passos aos 600 ms, escala tipográfica 80–200%, espelhamento árabe RTL e alfabetos africanos Adlam & N’Ko.',
    },
  },
  header: {
    spotlightTrigger: 'Spotlight HUD',
    spotlightShortcut: 'Alt+Space · Ctrl+K',
    audioOn: 'Kora 48 kHz Ativo',
    audioOff: 'Áudio Silenciado',
    themeDark: 'Obsidiana',
    themeLight: 'Pergaminho',
    accessibilityBtn: 'Ecrã & A11y',
    languageLabel: 'Idioma',
    githubLabel: 'Código-Fonte',
    menuOpen: 'Abrir navegação',
    menuClose: 'Fechar navegação',
  },
  footer: {
    manifestoLine: 'O sistema que nada esquece e diz sempre o que fez.',
    productHeading: 'Sistemas & Engenharia',
    trustHeading: 'Centro Jurídico, Privacidade & Confiança',
    engineeringHeading: 'Pegada Verificada do Repositório',
    zeroExfiltrationBadge: '0 cookies · 0 rastreadores · fontes OFL locais SHA-256',
    preAlphaDisclaimer:
      'Estado Pré-Alfa (v0.1.0): protótipo aberto fornecido "AS IS". Não utilize ainda com segredos críticos de produção.',
    etymologyNote:
      'Em Pulaar (Fula), "hakille" significa inteligência atenta, discernimento e sabedoria responsável.',
    copyrightLine: '© 2026 Projeto Hakille · Código Apache-2.0 · Constituição v1.0.0',
  },
  a11yDrawer: {
    title: 'Soberania Visual, Acústica e de Acessibilidade',
    subtitle: 'As preferências ficam exclusivamente no seu navegador (zero cookies transmitidos).',
    themeLabel: 'Material de superfície',
    contrastLabel: 'Modo de contraste WCAG',
    contrastNormal: 'Padrão AAA (17:1)',
    contrastHigh: 'Contraste máximo',
    textScaleLabel: 'Escala tipográfica (80% – 200%)',
    motionLabel: 'Cinética & cordas',
    motionNormal: 'Física elástica & onda',
    motionReduced: 'Movimento reduzido calmo',
    soundLabel: 'Síntese Karplus-Strong (48 kHz)',
    soundHint: 'Silenciado por defeito. Ativa as cordas dedilhadas matemáticas de sound.rs.',
    resetBtn: 'Repor preferências',
    closeBtn: 'Fechar',
  },
  home: {
    heroMeta: 'PRÉ-ALFA v0.1.0 · RUST EDITION 2024 · APACHE-2.0 · ZERO PYTHON',
    heroTitle: 'O sistema que nada esquece e prova sempre o que fez.',
    heroSubtitle:
      'Hakille é um sistema operativo cognitivo soberano escrito em Rust puro. Não é um chatbot: a sua memória vive num grafo local auditável, ações irreversíveis exigem 600 ms de retenção física e cada decisão é assinada num livro-razão criptográfico.',
    heroPrimaryCta: 'Testar Retenção 600 ms & Auditoria',
    heroSecondaryCta: 'Inspecionar Matriz de Verdade v0.1',
    heroSpotlightPrompt: 'Invocar Spotlight HUD (:status, :models-honest, :powers…)',
    metricsPackages: '41 crates Cargo',
    metricsLoc: '122 639 linhas Rust',
    metricsTests: '1 468 testes automáticos',
    metricsPython: '0% Python · 0% Electron',

    act2Eyebrow: 'ATO II · A CONTRATESE ATENCIONAL',
    act2Title: 'A interface que triunfa quando desaparece.',
    act2Body:
      'As IAs convencionais otimizam minutos presos numa janela de chat. O Hakille inverte a métrica principal: quanto melhor compreende o seu contexto, menos tempo passa diante do ecrã.',
    act2IndustryTitle: 'Chatbots na Nuvem Convencionais',
    act2IndustryPoints: [
      'Janela de chat permanente desenhada para maximizar a retenção',
      'Memória pessoal opaca trancada em servidores de terceiros',
      'Vulnerabilidade crónica a injeções ocultas em e-mails externos',
      'Zero prova criptográfica do que foi realmente executado',
    ],
    act2HakilleTitle: 'Arquitetura Ambiente Hakille',
    act2HakillePoints: [
      '0 fps em repouso · HUD Spotlight 720×64 que evapora em 1 500 ms',
      'Grafo SPO redb local exportável para JSON v1 aberto a qualquer momento',
      'Etiqueta imutável Untrusted que bloqueia cargas externas',
      'Diário SHA-256 + Ed25519 encadeado com folha Porquê? de 3 níveis',
    ],

    act3Eyebrow: 'ATO III · LABORATÓRIO CRIPTOGRÁFICO AO VIVO',
    act3Title: '600 milissegundos contra o irreversível. E uma cadeia SHA-256 incorruptível.',
    act3Body:
      'A cor Brasa (#FF7A2F) é proibida em 99,5% da interface: acende-se apenas quando uma autorização humana é exigida. Teste abaixo um ataque por e-mail, mantenha o botão Brasa por 600 ms e tente adulterar um bloco assinado.',

    act4Eyebrow: 'ATO IV · MEMÓRIA SOBERANA & CONFORMIDADE AI ACT UE',
    act4Title: 'A sua identidade vive no grafo SPO, nunca nos pesos neurais.',
    act4Body:
      'Os factos residem em partições estanques (Pessoal, Profissional, Partilhada). Os sinais afetivos ou posturais (cognition:*) têm semivida de 5 minutos seguida de purga física sem tombstone.',

    act5Eyebrow: 'ATO V · ARQUITETURA DE 6 CAMADAS & COGNITIVE FABRIC',
    act5Title: '122 639 linhas de Rust puro. E a nuvem presa numa trela de 300 segundos.',
    act5Body:
      'Substitua 5 modelos especializados por 1 modelo omni num manifesto TOML sem perder uma única memória. Quando o Sistema 2 escala para o seu endpoint BYOM, um token Biscuit de 300 s limita bytes e cêntimos.',

    act6Eyebrow: 'ATO VI · SOBRIEDADE MULTIMODAL & AUTONOMIA CONQUISTADA',
    act6Title: 'Três créditos proativos por dia. Cinco níveis que recuam ao menor desfazer.',
    act6Body:
      'Simule o ruído ambiente, o silêncio em reunião ou a progressão do Nível 0 ao Nível 4 — com despromoção instantânea assim que anula uma ação.',

    act7Eyebrow: 'ATO VII · DO MICROKERNEL REDOX AO COMPANHEIRO MÓVEL',
    act7Title: 'Um cérebro pessoal em três patamares honestos de hardware.',
    act7Body:
      'O Hakille nunca finge substituir o kernel do iOS. Expõe garantias exatas entre Patamar 1 Soberano (Redox/Linux), Patamar 2 Nativo (winit + wgpu + AccessKit) e Patamar 3 Convidado (SwiftUI / Compose via UniFFI com Checkpoint v2).',

    act8Eyebrow: 'ATO VIII · GOVERNAÇÃO & ESCRITAS DO MUNDO',
    act8Title: 'Uma Constituição inviolável acima do modelo. Enraizada na pluralidade linguística.',
    act8Body:
      'Dez regras verificáveis por máquina executam antes da descodificação neural — da proteção local de menores em <1 ms ao protocolo humano de crise (3114 / 112 / 988).',

    act9Eyebrow: 'ATO IX · CONTRATO PRÉ-ALFA & ARRANQUE RÁPIDO',
    act9Title: 'Verifique cada afirmação com cargo test.',
    act9Body:
      'Sem listas de espera artificiais nem vídeos encenados: clone o monorepo Apache-2.0, execute a suíte de conformidade e audite cada crate.',
    act9CopyCmd: 'Copiar',
    act9Copied: 'Copiado',
  },
  kora: {
    title: 'O Brilho Sob o Arco de 21 Fios',
    subtitle: 'Instrumento de Presença (brand.rs · 21 cordas de Kora · síntese Karplus-Strong)',
    stateIdle: 'Repouso (0 fps)',
    stateListening: 'Escuta',
    stateThinking: 'Reflexão',
    stateSpeaking: 'Fala (21 fios)',
    stateDecision: 'Decisão Brasa',
    stateEmergency: 'Apoio Calmo',
    bargeInBtn: 'Interromper fala (Barge-In < 80 ms)',
    bargeInStatus: 'Barge-In concluído em 42 ms: onda recolhida sem estalidos.',
    pluckHint: 'Passe o cursor ou clique nas 21 cordas verticais da Kora para dedilhar.',
    fpsZeroBadge: 'Loop GPU congelado · 0.0 fps',
    fpsActiveBadge: 'Onda ativa · 21 cordas',
  },
  spotlight: {
    placeholder: 'Digite um comando soberano (:status, :models-honest, :powers, :budget) ou salte para uma página…',
    hintBar: 'Enter para executar · Setas para escolher · Esc para evaporar',
    closeEsc: 'Esc',
    emptyPrompt: 'Comandos soberanos instantâneos:',
    runBtn: 'Executar',
  },
  actionLab: {
    scenarioInjectionTab: '1. E-mail Armadilhado (Untrusted)',
    scenarioTransferTab: '2. Transferir 25 € (Irreversível)',
    scenarioArchiveTab: '3. Arquivar Fio (Reversível)',
    untrustedBadge: '[externo · Untrusted]',
    trustedBadge: '[intenção humana verificada]',
    costPill: 'Custo: 0,00 € (Local)',
    dataPill: 'Dados: 0 bytes enviados',
    revPillIrreversible: 'Irreversível · Retenção 600 ms',
    revPillReversible: 'Reversível em 1 clique',
    holdButtonLabel: 'Manter 600 ms para selar',
    holdHoldingLabel: 'A manter…',
    holdArmedLabel: 'Passo 1/2 armado — Prima novamente para confirmar',
    holdConfirmedLabel: 'Selado & Assinado Ed25519',
    holdCancelledHint: 'Libertado antes de 600 ms: zero efeitos executados.',
    accessibleModeToggle: 'Modo acessível por teclado (2 pressões sem temporizador)',
    blockedByRule01:
      'BLOQUEIO DURO · RULE-01: Carga proveniente de TrustLevel::Untrusted. A confirmação irreversível está estruturalmente desativada e registada.',
    executeReversibleBtn: 'Executar arquivo reversível (sem retenção)',
    auditHeading: 'Diário Criptográfico de Auditoria ao Vivo',
    chainValidBadge: 'Cadeia SHA-256 + Assinaturas Íntegras',
    chainBrokenBadge: 'ALERTA: Adulteração Criptográfica Detetada!',
    tamperBtn: 'Simular adulteração de 1 byte',
    restoreChainBtn: 'Restaurar diário íntegro',
    whyBtn: 'Porquê? (3 níveis)',
    undoBtn: 'Anular ação',
    whyLevel1: 'Nível 1 · Explicação clara',
    whyLevel2: 'Nível 2 · Factos SPO consultados',
    whyLevel3: 'Nível 3 · Prova técnica, Biscuit & Contrafactual',
    markFactFalseBtn: 'Este facto é falso',
    factDisputedBadge: 'Invalidado por si',
  },
  memoryLab: {
    partitionPersonal: 'Partição Pessoal',
    partitionPro: 'Partição Profissional',
    partitionShared: 'Partilhada Familiar',
    timeSliderLabel: 'Tempo decorrido (decaimento biológico & purga AI Act UE)',
    timeNow: 't = 0 min (Agora)',
    time5m: 't = 5 min (Purga AI Act)',
    time30d: 't = 30 dias',
    time2y: 't = 2 anos',
    aiActNotice:
      'Conformidade AI Act UE & Regra 8: em t >= 5 min, os sinais afetivos/posturais (cognition:*) são fisicamente purgados sem tombstones.',
    exportJsonBtn: 'Transferir hakille-graph-export JSON v1',
    digitalTwilightTitle: 'Pacto Sucessório Crepúsculo Digital',
    digitalTwilightBody:
      'Defina em vida, por diretiva assinada, se a inatividade prolongada aciona apagamento criptográfico ou transmissão seletiva.',
  },
  fabricLab: {
    layersTab: 'Corte de 6 Camadas',
    cratesTab: 'Explorador de 41 Crates',
    system2Tab: 'Simulador Biscuit 300 s (Sistema 2)',
    searchCratesPlaceholder: 'Filtrar crate ou tecnologia (wgpu, redb, biscuit, whisper)…',
    allLayersFilter: 'Todas as camadas (41)',
    localSystem1Title: 'Sistema 1 · Inferência Local Offline',
    remoteSystem2Title: 'Sistema 2 · Escalada BYOM sob Licença Biscuit 300 s',
    issueBiscuitBtn: 'Emitir licença Biscuit 300 s',
    simulateStreamBtn: 'Simular turno remoto (+1,8 KB / +0,004 €)',
    revokeTokenBtn: 'Revogar token imediatamente',
  },
  ambientLab: {
    channelHeading: 'Seletor Multimodal de Canal',
    noiseLabel: 'Ruído ambiente medido',
    meetingToggle: 'Em reunião (calendário / microfone)',
    thirdPartyToggle: 'Terceiros presentes na sala',
    headphonesToggle: 'Auscultadores privados ligados',
    silenceBudgetHeading: 'Orçamento de Silêncio (3 créditos / dia)',
    triggerNudgeBtn: 'Disparar sugestão proativa',
    dismissTopicBtn: 'Ignorar tema (pausa de 14 dias)',
    resetDayBtn: 'Reiniciar dia',
    autonomyHeading: 'Escada de Autonomia por Domínio (Níveis 0 → 4)',
    validate20Btn: '+5 ações validadas sem erro',
    promoteCrankBtn: 'Aceitar subida de nível',
    humanUndoBtn: 'Anular última ação (Despromoção instantânea -1)',
  },
  inspector: {
    title: 'Inspetor ao Vivo Zero-Exfiltração',
    subtitle: 'Executado diretamente no seu navegador sobre este documento.',
    cookiesLabel: 'Cookies gravados (document.cookie)',
    thirdPartyReqLabel: 'Pedidos a domínios terceiros (Performance API)',
    fontShaLabel: 'Fonte local InterVariable.ttf (SIL OFL 1.1)',
    storageKeysLabel: 'Chaves locais (localStorage)',
    rescanBtn: 'Repetir auditoria ao vivo',
    wipeLocalBtn: 'Limpar preferências locais em 1 clique',
    wipedConfirm: 'Preferências locais apagadas; padrões restaurados.',
  },
  common: {
    readFullDoc: 'Explorar especificação completa',
    verifiedInRepo: 'Verificado no monorepo em HEAD',
    preAlphaWarningTitle: 'Honestidade Pré-Alfa v0.1.0',
    preAlphaWarningBody:
      'O Hakille documenta abertamente o que corre hoje de ponta a ponta face ao que permanece não aprovisionado.',
    plainSummaryTitle: 'Resumo Claro (30 segundos)',
    lastUpdated: 'Última atualização',
    filterAll: 'Todos',
  },
};
