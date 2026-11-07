import { TranslationSchema } from '../types';

export const esLocale: TranslationSchema = {
  meta: {
    brandSignature: 'Hakille — inteligencia soberana',
    preAlphaBadge: 'Pre-Alfa v0.1.0 · Apache-2.0 · 100 % Rust',
    skipToContent: 'Saltar directamente al contenido principal',
  },
  routes: {
    home: {
      navLabel: 'Manifiesto',
      title: 'hakille — inteligencia soberana · Sistema operativo cognitivo en Rust',
      description:
        'El sistema que no olvida nada y siempre demuestra lo que hizo. Grafo local SPO, capacidades Biscuit, pulsación física de 600 ms y diario de auditoría SHA-256 + Ed25519.',
    },
    architecture: {
      navLabel: 'Arquitectura',
      title: 'Arquitectura de 6 Capas y 41 Crates en Rust — hakille',
      description:
        'Explora las 6 capas de Hakille: 122 639 líneas de Rust 2024, 0 % Python, shell nativo GPU winit+wgpu+AccessKit, Wasmtime 48 y Cognitive Fabric.',
    },
    honestStatus: {
      navLabel: 'Estado Real v0.1',
      title: 'Matriz de Honestidad Pre-Alfa v0.1 — hakille',
      description:
        'Transparencia radical entre lo que hoy está verificado (1 468 pruebas, Whisper ×46 CPU, Mimi, Qwen GGUF) y la hoja de ruta Fase 1–3.',
    },
    security: {
      navLabel: 'Seguridad',
      title: 'Capacidades Biscuit, Aislamiento Untrusted y Seguridad — hakille',
      description:
        'Tokens Biscuit v5 con atenuación monótona, inmunidad frente a inyección de prompts, almacén HAKKEY02 Argon2id y divulgación RFC 9116 (72h).',
    },
    constitution: {
      navLabel: 'Constitución',
      title: 'Constitución Inviolable v1.0.0 — hakille',
      description:
        'Los 7 Artículos y 10 Reglas verificables por máquina por encima de cualquier modelo: umbrales de confianza, protección de menores y protocolo de crisis.',
    },
    install: {
      navLabel: 'Instalar',
      title: 'Dimensionador RAM, Sustratos OS y Compilación Rust — hakille',
      description:
        'Calcule la huella exacta de memoria local de Hakille en su equipo (2 GB a 16 GB+) y compile el demonio, el escritorio winit+wgpu o el puente móvil UniFFI.',
    },
    privacy: {
      navLabel: 'Privacidad y RGPD',
      title: 'Política de Privacidad RGPD, ePrivacy y AI Act — hakille',
      description:
        'Doble garantía soberana: cero cookies, cero rastreadores y fuentes OFL locales en hakille.ai; memoria local redb y purga física de 5 min en Hakille OS.',
    },
    legalNotice: {
      navLabel: 'Aviso Legal',
      title: 'Aviso Legal (LCEN) y Gobernanza Abierta — hakille',
      description:
        'Identificación del proyecto open-source Hakille, canales oficiales de contacto, alojamiento estático soberano y propiedad intelectual.',
    },
    conditions: {
      navLabel: 'Condiciones',
      title: 'Condiciones de Uso y Aviso de Prototipo Pre-Alfa — hakille',
      description:
        'Condiciones de uso de hakille.ai y cláusulas esenciales TAL CUAL (AS IS) para probar el prototipo pre-alfa v0.1.',
    },
    licenses: {
      navLabel: 'Licencias y Patentes',
      title: 'Licencia Apache-2.0, Patentes Defensivas y Procedencia SHA-256 — hakille',
      description:
        'Licencia Apache-2.0, compromiso defensivo de patentes, auditoría cargo-deny sin copyleft y hashes SHA-256 de fuentes SIL OFL 1.1.',
    },
    accessibility: {
      navLabel: 'Accesibilidad',
      title: 'Accesibilidad WCAG AAA/AA, Teclado, RTL y Escrituras — hakille',
      description:
        'Contraste 17:1 AAA, confirmación accesible en 2 pasos, escala 80–200 %, diseño RTL árabe y escrituras Adlam y N’Ko.',
    },
  },
  header: {
    spotlightTrigger: 'Spotlight HUD',
    spotlightShortcut: 'Alt+Espacio · Ctrl+K',
    audioOn: 'Kora 48 kHz activo',
    audioOff: 'Audio silenciado',
    themeDark: 'Obsidiana',
    themeLight: 'Pergamino',
    accessibilityBtn: 'Pantalla y A11y',
    languageLabel: 'Idioma',
    githubLabel: 'Código Fuente',
    menuOpen: 'Abrir navegación',
    menuClose: 'Cerrar navegación',
  },
  footer: {
    manifestoLine: 'El sistema que no olvida nada y siempre dice lo que hizo.',
    productHeading: 'Ingeniería y Sistema',
    trustHeading: 'Centro Legal, Privacidad y Confianza',
    engineeringHeading: 'Huella Verificada del Repositorio',
    zeroExfiltrationBadge: '0 cookies · 0 rastreadores · Fuentes OFL locales SHA-256',
    preAlphaDisclaimer:
      'Estado Pre-Alfa (v0.1.0): prototipo abierto entregado «TAL CUAL». No lo use aún con secretos de producción ni pagos reales.',
    etymologyNote:
      'En pulaar (fulani), «hakille» significa inteligencia viva, discernimiento y sabiduría responsable.',
    copyrightLine: '© 2026 Proyecto Hakille · Código bajo Apache-2.0 · Constitución v1.0.0',
  },
  a11yDrawer: {
    title: 'Soberanía Visual, Acústica y Accesibilidad',
    subtitle: 'Preferencias guardadas solo en su navegador (cero cookies enviadas).',
    themeLabel: 'Material de superficie',
    contrastLabel: 'Modo de contraste WCAG',
    contrastNormal: 'Estándar AAA (17:1)',
    contrastHigh: 'Contraste máximo',
    textScaleLabel: 'Escala tipográfica (80 % – 200 %)',
    motionLabel: 'Cinética y cuerdas',
    motionNormal: 'Física de resorte y onda',
    motionReduced: 'Movimiento reducido calmo',
    soundLabel: 'Síntesis Karplus-Strong (48 kHz)',
    soundHint: 'Silenciado por defecto. Activa las cuerdas pulsadas matemáticas de sound.rs.',
    resetBtn: 'Restablecer ajustes',
    closeBtn: 'Cerrar',
  },
  home: {
    heroMeta: 'PRE-ALFA v0.1.0 · RUST EDICIÓN 2024 · APACHE-2.0 · CERO PYTHON',
    heroTitle: 'El sistema que no olvida nada y siempre demuestra lo que hizo.',
    heroSubtitle:
      'Hakille es un sistema operativo cognitivo soberano escrito en Rust puro. No es un chatbot: su memoria vive en un grafo local auditable, toda acción irreversible exige mantener 600 ms y cada decisión se firma criptográficamente.',
    heroPrimaryCta: 'Probar Laboratorio de 600 ms y Auditoría',
    heroSecondaryCta: 'Ver Matriz de Honestidad v0.1',
    heroSpotlightPrompt: 'Invocar Spotlight HUD (:status, :models-honest, :powers…)',
    metricsPackages: '41 crates Cargo',
    metricsLoc: '122 639 líneas Rust',
    metricsTests: '1 468 pruebas',
    metricsPython: '0 % Python · 0 % Electron',

    act2Eyebrow: 'ACTO II · CONTRATESIS DE LA ATENCIÓN',
    act2Title: 'La interfaz que triunfa cuando desaparece.',
    act2Body:
      'La industria mide su éxito por el tiempo atrapado en una ventana de chat. Hakille invierte la métrica norte: cuanto mejor le comprende, menos pantalla necesita.',
    act2IndustryTitle: 'Chatbots en la Nube Convencionales',
    act2IndustryPoints: [
      'Ventana de chat permanente diseñada para retener su atención',
      'Memoria personal encerrada en servidores opacos de terceros',
      'Vulnerabilidad estructural ante inyecciones de prompts en correos',
      'Cero pruebas criptográficas de lo que realmente se ejecutó',
    ],
    act2HakilleTitle: 'Arquitectura Ambiental Hakille',
    act2HakillePoints: [
      '0 fps en reposo · HUD Spotlight 720×64 que se evapora en 1 500 ms',
      'Grafo local SPO redb exportable a JSON v1 abierto en cualquier momento',
      'Etiqueta inmutable Untrusted que bloquea cargas externas',
      'Diario encadenado SHA-256 + Ed25519 con Hoja «¿Por qué?» de 3 niveles',
    ],

    act3Eyebrow: 'ACTO III · LABORATORIO CRIPTOGRÁFICO EN VIVO',
    act3Title: '600 milisegundos frente a lo irreversible. Y una cadena SHA-256 inalterable.',
    act3Body:
      'El color Brasa (#FF7A2F) está prohibido en el 99,5 % del sistema: solo se enciende ante una decisión humana. Pruebe un correo malicioso, mantenga pulsado 600 ms y trate de alterar un bloque firmado por WebCrypto.',

    act4Eyebrow: 'ACTO IV · MEMORIA SOBERANA Y AI ACT EUROPEO',
    act4Title: 'Su identidad habita su grafo SPO, jamás los pesos de un modelo.',
    act4Body:
      'Los hechos viven en particiones estancas (Personal, Profesional, Compartido). Las señales afectivas (cognition:*) se purgan físicamente sin rastro a los 5 minutos.',

    act5Eyebrow: 'ACTO V · 6 CAPAS EN RUST Y COGNITIVE FABRIC',
    act5Title: '122 639 líneas de Rust puro. Y la nube sujeta con correa de 300 segundos.',
    act5Body:
      'Cambie modelos editando un manifiesto TOML sin perder memoria. Cuando el Sistema 2 escala a su propio servidor (BYOM), un token Biscuit de 300 s limita bytes y céntimos.',

    act6Eyebrow: 'ACTO VI · SOBRIEDAD MULTIMODAL Y AUTONOMÍA GANADA',
    act6Title: 'Tres créditos diarios de silencio. Cinco niveles que bajan al primer deshacer.',
    act6Body:
      'Simule decibelios ambientales, reuniones o el ascenso del Nivel 0 al Nivel 4 con degradación automática inmediata si revierte una acción.',

    act7Eyebrow: 'ACTO VII · DEL MICRONÚCLEO REDOX AL MÓVIL',
    act7Title: 'Un cerebro personal en tres niveles honestos de hardware.',
    act7Body:
      'Hakille detalla garantías reales entre Nivel 1 Soberano (Redox/Linux), Nivel 2 Nativo (winit + wgpu + AccessKit) y Nivel 3 Invitado (SwiftUI / Compose vía UniFFI y Checkpoint v2).',

    act8Eyebrow: 'ACTO VIII · GOBERNANZA Y ESCRITURAS DEL MUNDO',
    act8Title: 'Una Constitución por encima del modelo. Enraizada en la pluralidad lingüística.',
    act8Body:
      'Diez reglas verificables por máquina protegen menores en < 1 ms y activan ayuda humana calmada en crisis (3114 / 112 / 988).',

    act9Eyebrow: 'ACTO IX · CONTRATO DE HONESTIDAD Y ARRANQUE RÁPIDO',
    act9Title: 'Verifique cada afirmación con cargo test.',
    act9Body:
      'Sin listas de espera artificiales: clone el monorepo Apache-2.0, ejecute las pruebas de conformidad e inspeccione cada crate.',
    act9CopyCmd: 'Copiar',
    act9Copied: 'Copiado',
  },
  kora: {
    title: 'El Resplandor bajo el arco de 21 hilos',
    subtitle: 'Instrumento de Presencia (brand.rs · 21 cuerdas de kora · síntesis Karplus-Strong)',
    stateIdle: 'Reposo (0 fps)',
    stateListening: 'Escucha',
    stateThinking: 'Reflexión',
    stateSpeaking: 'Voz (21 hilos)',
    stateDecision: 'Decisión Brasa',
    stateEmergency: 'Auxilio calmo',
    bargeInBtn: 'Interrumpir voz (Barge-In < 80 ms)',
    bargeInStatus: 'Barge-In completado en 42 ms: onda replegada al instante.',
    pluckHint: 'Pase el cursor o pulse las 21 cuerdas de kora (active el audio de 48 kHz arriba para oírlas).',
    fpsZeroBadge: 'Bucle GPU detenido · 0,0 fps',
    fpsActiveBadge: 'Onda activa · 21 cuerdas',
  },
  spotlight: {
    placeholder: 'Escriba un comando (:status, :models-honest, :powers, :budget, :journal-why 1) o salte a una página…',
    hintBar: 'Intro para ejecutar · Flechas para elegir · Esc para evaporar',
    closeEsc: 'Esc',
    emptyPrompt: 'Comandos soberanos instantáneos:',
    runBtn: 'Ejecutar',
  },
  actionLab: {
    scenarioInjectionTab: '1. Correo trampa (Untrusted)',
    scenarioTransferTab: '2. Transferir 25 € (Irreversible)',
    scenarioArchiveTab: '3. Archivar hilo (Reversible)',
    untrustedBadge: '[externo · Untrusted]',
    trustedBadge: '[intención humana verificada]',
    costPill: 'Coste: 0,00 € (Local)',
    dataPill: 'Datos: 0 bytes salientes',
    revPillIrreversible: 'Irreversible · Mantener 600 ms',
    revPillReversible: 'Reversible en 1 clic',
    holdButtonLabel: 'Mantener 600 ms para sellar',
    holdHoldingLabel: 'Manteniendo…',
    holdArmedLabel: 'Paso 1/2 armado — Pulse de nuevo para confirmar',
    holdConfirmedLabel: 'Sellado y firmado Ed25519',
    holdCancelledHint: 'Soltado antes de 600 ms: cero efectos ejecutados.',
    accessibleModeToggle: 'Modo teclado accesible (2 pulsaciones explícitas)',
    blockedByRule01:
      'HARD BLOCK · RULE-01: Contenido procedente de TrustLevel::Untrusted. La confirmación irreversible está deshabilitada estructuralmente y registrada.',
    executeReversibleBtn: 'Ejecutar archivo reversible (sin espera)',
    auditHeading: 'Diario de Auditoría Criptográfica en Vivo',
    chainValidBadge: 'Cadena SHA-256 + Firmas Íntegras',
    chainBrokenBadge: '¡ALERTA: Alteración criptográfica detectada!',
    tamperBtn: 'Simular alteración de 1 byte',
    restoreChainBtn: 'Restaurar diario íntegro',
    whyBtn: '¿Por qué? (3 niveles)',
    undoBtn: 'Deshacer',
    whyLevel1: 'Nivel 1 · Explicación clara en una frase',
    whyLevel2: 'Nivel 2 · Hechos SPO consultados',
    whyLevel3: 'Nivel 3 · Prueba técnica, Biscuit y Contrafactual',
    markFactFalseBtn: 'Este hecho es falso',
    factDisputedBadge: 'Invalidado por usted',
  },
  memoryLab: {
    partitionPersonal: 'Partición Personal',
    partitionPro: 'Partición Profesional',
    partitionShared: 'Hogar Compartido',
    timeSliderLabel: 'Paso del tiempo (olvido biológico y purga AI Act)',
    timeNow: 't = 0 min (Ahora)',
    time5m: 't = 5 min (Purga AI Act)',
    time30d: 't = 30 días',
    time2y: 't = 2 años',
    aiActNotice:
      'Cumplimiento AI Act UE y Regla 8: en t >= 5 min, las señales afectivas (cognition:*) se purgan físicamente sin dejar tombstone.',
    exportJsonBtn: 'Descargar hakille-graph-export JSON v1',
    digitalTwilightTitle: 'Pacto «Crepúsculo Digital»',
    digitalTwilightBody:
      'Defina en vida mediante directiva firmada si la inactividad prolongada destruye criptográficamente su grafo o transfiere una selección a un legatario.',
  },
  fabricLab: {
    layersTab: 'Corte de 6 Capas',
    cratesTab: 'Explorador de 41 Crates',
    system2Tab: 'Simulador Biscuit 300 s (Sistema 2)',
    searchCratesPlaceholder: 'Filtrar por crate o tecnología (wgpu, redb, biscuit, whisper)…',
    allLayersFilter: 'Todas las capas (41)',
    localSystem1Title: 'Sistema 1 · Inferencia Local sin Red',
    remoteSystem2Title: 'Sistema 2 · Escalado BYOM bajo Token Biscuit de 300 s',
    issueBiscuitBtn: 'Emitir token Biscuit 300 s',
    simulateStreamBtn: 'Simular consulta (+1,8 KB / +0,004 €)',
    revokeTokenBtn: 'Revocar token al instante',
  },
  ambientLab: {
    channelHeading: 'Selector Multimodal de Canal',
    noiseLabel: 'Ruido ambiental medido',
    meetingToggle: 'En reunión (calendario / micro)',
    thirdPartyToggle: 'Tercero presente en la sala',
    headphonesToggle: 'Auriculares privados conectados',
    silenceBudgetHeading: 'Presupuesto de Silencio (3 créditos / día)',
    triggerNudgeBtn: 'Emitir sugerencia proactiva',
    dismissTopicBtn: 'Descartar tema (Silencio 14 días)',
    resetDayBtn: 'Nuevo día',
    autonomyHeading: 'Escala de Autonomía por Dominio (Niveles 0 → 4)',
    validate20Btn: '+5 acciones validadas sin fallo',
    promoteCrankBtn: 'Aceptar ascenso de nivel',
    humanUndoBtn: 'Deshacer acción (Bajada inmediata -1 nivel)',
  },
  inspector: {
    title: 'Inspector en Vivo «Cero Exfiltración»',
    subtitle: 'Verificación ejecutada en tiempo real dentro de su navegador.',
    cookiesLabel: 'Cookies almacenadas (document.cookie)',
    thirdPartyReqLabel: 'Peticiones a dominios externos (Performance API)',
    fontShaLabel: 'Fuente local InterVariable.ttf (SIL OFL 1.1)',
    storageKeysLabel: 'Claves locales (localStorage)',
    rescanBtn: 'Repetir auditoría',
    wipeLocalBtn: 'Borrar preferencias locales en 1 clic',
    wipedConfirm: 'Preferencias locales borradas; valores iniciales restaurados.',
  },
  common: {
    readFullDoc: 'Explorar especificación completa',
    verifiedInRepo: 'Verificado en el monorepo en HEAD',
    preAlphaWarningTitle: 'Honestidad Pre-Alfa v0.1.0',
    preAlphaWarningBody:
      'Hakille distingue con total franqueza qué funciona hoy de extremo a extremo y qué sigue en integración.',
    plainSummaryTitle: 'Resumen claro (30 segundos)',
    lastUpdated: 'Última actualización',
    filterAll: 'Todos',
  },
};
