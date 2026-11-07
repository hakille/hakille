import { TranslationSchema } from '../types';

export const arLocale: TranslationSchema = {
  meta: {
    brandSignature: 'هاكيلي — ذكاء سيادي',
    preAlphaBadge: 'ما قبل ألفا v0.1.0 · Apache-2.0 · 100% Rust',
    skipToContent: 'انتقل مباشرة إلى المحتوى الرئيسي',
  },
  routes: {
    home: {
      navLabel: 'البيان',
      title: 'hakille — ذكاء سيادي · نظام تشغيل معرفي بلغة Rust النقية',
      description:
        'النظام الذي لا ينسى شيئًا ويثبت دائمًا ما فعله. رسم بياني محلي للذاكرة SPO، وصلاحيات Biscuit، وضغطة تأكيد 600 مللي ثانية، وسجل تدقيق SHA-256 + Ed25519.',
    },
    architecture: {
      navLabel: 'البنية المعمارية',
      title: 'البنية ذات الطبقات الست و41 حزمة Rust — hakille',
      description:
        'استكشف طبقات Hakille الست: 122,639 سطرًا من Rust 2024، 0% بايثون، واجهة رسومية أصلية winit+wgpu+AccessKit، وصندوق رمل Wasmtime 48.',
    },
    honestStatus: {
      navLabel: 'الحالة الفعلية v0.1',
      title: 'مصفوفة الشفافية لإصدار ما قبل ألفا v0.1 — hakille',
      description:
        'شفافية هندسية كاملة تفصل بين ما يعمل فعليًا اليوم عبر 1,468 اختبارًا وبين خارطة الطريق للمراحل القادمة.',
    },
    security: {
      navLabel: 'الأمان',
      title: 'صلاحيات Biscuit وعزل المحتوى الخارجي وسياسة الأمان — hakille',
      description:
        'رموز Biscuit v5 ذات التقييد الأحادي، ومناعة بنيوية ضد حقن الأوامر، وخزنة مفاتيح HAKKEY02، وإفصاح أمني خلال 72 ساعة.',
    },
    constitution: {
      navLabel: 'الدستور',
      title: 'الدستور غير القابل للخرق v1.0.0 — hakille',
      description:
        'المواد السبع والقواعد العشر القابلة للتحقق آليًا فوق كل نموذج: عتبات الثقة، وحماية القُصّر، وبروتوكول الطوارئ الإنسانية.',
    },
    install: {
      navLabel: 'التثبيت',
      title: 'حاسبة الذاكرة المحلية وأنظمة التشغيل وبناء Rust — hakille',
      description:
        'احسب البصمة الدقيقة للذاكرة المحلية لنظام Hakille على جهازك (من 2 إلى 16+ غيغابايت) وابنِ الخادم المحلي أو واجهة سطح المكتب والهاتف.',
    },
    privacy: {
      navLabel: 'الخصوصية وGDPR',
      title: 'سياسة الخصوصية وفق GDPR وePrivacy وقانون الذكاء الاصطناعي الأوروبي — hakille',
      description:
        'ضمان مزدوج للسيادة: صفر ملفات تعريف ارتباط وصفر تتبع على hakille.ai؛ وذاكرة محلية مع مسح فعلي للإشارات الانفعالية خلال 5 دقائق.',
    },
    legalNotice: {
      navLabel: 'الإشعار القانوني',
      title: 'الإشعار القانوني والحوكمة مفتوحة المصدر — hakille',
      description:
        'تعريف مشروع Hakille مفتوح المصدر، وقنوات الاتصال الرسمية، وبنية الاستضافة الثابتة، وحقوق الملكية الفكرية.',
    },
    conditions: {
      navLabel: 'الشروط',
      title: 'شروط الاستخدام وتنبيه النموذج الأولي ما قبل ألفا — hakille',
      description:
        'شروط استخدام موقع hakille.ai وإخلاء المسؤولية الأساسي لاستخدام النسخة التجريبية v0.1 «كما هي».',
    },
    licenses: {
      navLabel: 'التراخيص والبراءات',
      title: 'رخصة Apache-2.0 والتعهد الدفاعي للبراءات وبصمات SHA-256 — hakille',
      description:
        'ترخيص Apache-2.0، والتعهد الدفاعي ببراءات الاختراع، وفحص cargo-deny، والتحقق من خطوط SIL OFL 1.1 عبر SHA-256.',
    },
    accessibility: {
      navLabel: 'إمكانية الوصول',
      title: 'معايير WCAG AAA/AA ودعم RTL والأبجديات العالمية — hakille',
      description:
        'تباين 17:1 AAA، وبديل لوحة المفاتيح لضغطة 600 مللي ثانية، وتكبير النص 80–200%، ودعم العربية RTL وأبجديتي أدلام ونكو.',
    },
  },
  header: {
    spotlightTrigger: 'شريط Spotlight',
    spotlightShortcut: 'Alt+Space · Ctrl+K',
    audioOn: 'صوت الكورا 48kHz مفعّل',
    audioOff: 'الصوت مكتوم',
    themeDark: 'سبج داكن',
    themeLight: 'رقّ فاتح',
    accessibilityBtn: 'العرض والوصول',
    languageLabel: 'اللغة',
    githubLabel: 'المصدر البرمجي',
    menuOpen: 'فتح القائمة',
    menuClose: 'إغلاق القائمة',
  },
  footer: {
    manifestoLine: 'النظام الذي لا ينسى شيئًا ويقول دائمًا ما فعله.',
    productHeading: 'الهندسة والنظام',
    trustHeading: 'مركز الثقة والخصوصية والقانون',
    engineeringHeading: 'بصمة المستودع الموثّقة',
    zeroExfiltrationBadge: '0 كوكيز · 0 متتبعات · خطوط OFL محلية موثقة بـ SHA-256',
    preAlphaDisclaimer:
      'إصدار ما قبل ألفا (v0.1.0): نموذج هندسي مفتوح يُقدَّم «كما هو». لا تستخدمه بعد للأسرار الإنتاجية أو المدفوعات الحقيقية.',
    etymologyNote:
      'في لغة البولار (الفولاني)، تعني كلمة «hakille» الذكاء الحي والبصيرة والحكمة المسؤولة.',
    copyrightLine: '© 2026 مشروع Hakille · الشفرة برخصة Apache-2.0 · الدستور v1.0.0',
  },
  a11yDrawer: {
    title: 'السيادة البصرية والصوتية وإمكانية الوصول',
    subtitle: 'تُحفظ الإعدادات محليًا في متصفحك فقط دون إرسال أي ملفات ارتباط.',
    themeLabel: 'خامة السطح',
    contrastLabel: 'مستوى التباين WCAG',
    contrastNormal: 'قياسي AAA (17:1)',
    contrastHigh: 'تباين أقصى',
    textScaleLabel: 'مقياس الخط (80% – 200%)',
    motionLabel: 'الحركة والأوتار',
    motionNormal: 'فيزياء النابض والموجة',
    motionReduced: 'حركة مخفضة هادئة',
    soundLabel: 'توليد صوتي Karplus-Strong (48 kHz)',
    soundHint: 'مكتوم افتراضيًا. يفعّل أوتار الكورا الرياضية المولّدة مباشرة دون ملفات صوتية.',
    resetBtn: 'إعادة الضبط',
    closeBtn: 'إغلاق',
  },
  home: {
    heroMeta: 'ما قبل ألفا v0.1.0 · RUST 2024 · APACHE-2.0 · صفر بايثون',
    heroTitle: 'النظام الذي لا ينسى شيئًا ويثبت دائمًا ما فعله.',
    heroSubtitle:
      'هاكيلي نظام تشغيل معرفي سيادي مكتوب بالكامل بلغة Rust النقية. ليس روبوت محادثة: ذاكرتك تسكن رسمًا بيانيًا محليًا قابلًا للتدقيق، وكل فعل غير قابل للتراجع يتطلب ضغطة مستمرة لمدة 600 مللي ثانية وتوقيعًا رقميًا.',
    heroPrimaryCta: 'جرّب مختبر 600 مللي ثانية وسجل التدقيق',
    heroSecondaryCta: 'افحص مصفوفة الحقيقة v0.1',
    heroSpotlightPrompt: 'افتح شريط Spotlight (:status, :models-honest, :powers…)',
    metricsPackages: '41 حزمة Cargo',
    metricsLoc: '122,639 سطر Rust',
    metricsTests: '1,468 اختبارًا آليًا',
    metricsPython: '0% بايثون · 0% Electron',

    act2Eyebrow: 'الفصل الثاني · أطروحة مضادة لاقتصاد الانتباه',
    act2Title: 'الواجهة التي تنجح حين تختفي.',
    act2Body:
      'تقيس الأنظمة التقليدية نجاحها بعدد الدقائق التي تقضيها داخل نافذة الدردشة. يعكس هاكيلي البوصلة: كلما فهمك النظام أكثر، قلّ وقتك أمام الشاشة.',
    act2IndustryTitle: 'روبوتات الدردشة السحابية التقليدية',
    act2IndustryPoints: [
      'نافذة محادثة دائمة مصممة لاستبقاء المستخدم أطول وقت ممكن',
      'ذاكرة شخصية غامضة محتجزة في خوادم شركات خارجية',
      'هشاشة مزمنة أمام هجمات حقن الأوامر داخل الرسائل الواردة',
      'انعدام الإثبات التشفيري لما تم تنفيذه فعليًا ولماذا',
    ],
    act2HakilleTitle: 'البنية المحيطة في Hakille',
    act2HakillePoints: [
      '0 إطار/ثانية في السكون · شريط Spotlight يتلاشى خلال 1500 مللي ثانية',
      'رسم بياني محلي SPO قابل للتصدير بصيغة JSON v1 مفتوحة في أي لحظة',
      'وسم Untrusted غير قابل للتغيير يعزل المحتوى الخارجي بنيويًا',
      'سجل تدقيق متسلسل SHA-256 + Ed25519 مع ورقة «لماذا؟» من 3 مستويات',
    ],

    act3Eyebrow: 'الفصل الثالث · المختبر التشفيري الحي',
    act3Title: '600 مللي ثانية أمام ما لا رجعة فيه، وسلسلة SHA-256 لا تقبل التزوير.',
    act3Body:
      'لون الجمر (#FF7A2F) محظور في 99.5% من الواجهة ولا يضيء إلا عند القرار البشري. اختبر رسالة خبيثة أدناه، واضغط مطولًا لمدة 600 مللي ثانية، ثم حاول التلاعب بكتلة موقّعة عبر WebCrypto.',

    act4Eyebrow: 'الفصل الرابع · الذاكرة السيادية والامتثال لقانون الذكاء الاصطناعي الأوروبي',
    act4Title: 'هويتك تعيش في الرسم البياني SPO الخاص بك، لا داخل أوزان النموذج.',
    act4Body:
      'تُفصل الحقائق في أقسام معزولة (شخصي، مهني، مشترك)، وتخضع الإشارات الانفعالية (cognition:*) لمسح فعلي نهائي بعد 5 دقائق.',

    act5Eyebrow: 'الفصل الخامس · الطبقات الست ونسيج الإدراك',
    act5Title: '122,639 سطرًا من Rust النقية، والسحابة مقيّدة بعقد مدته 300 ثانية.',
    act5Body:
      'بدّل النماذج عبر ملف TOML دون فقدان الذاكرة، وعند الاستعانة بنموذج بعيد (النظام 2) يفرض رمز Biscuit مهلة 300 ثانية مع عدّاد تكلفة لحظي.',

    act6Eyebrow: 'الفصل السادس · رصانة الحواس والاستقلالية المكتسبة',
    act6Title: 'ثلاثة أرصدة صمت يوميًا، وخمس درجات استقلالية تتراجع فور أول إلغاء.',
    act6Body:
      'حاكِ مستوى الضجيج المحيط، أو وضع الاجتماعات، أو ترقية الوكيل من الدرجة 0 إلى 4 مع التراجع الآلي الفوري عند إلغاء أي إجراء.',

    act7Eyebrow: 'الفصل السابع · من نواة Redox المصغّرة إلى رفيق الهاتف',
    act7Title: 'عقل شخصي واحد عبر ثلاث مستويات عتادية صادقة.',
    act7Body:
      'يوضّح هاكيلي ضماناته بدقة بين المستوى السيادي (Redox/Linux)، والمستوى الأصلي (winit + wgpu + AccessKit)، ومستوى الضيف المحمول (UniFFI مع Checkpoint v2).',

    act8Eyebrow: 'الفصل الثامن · الدستور وتعددية أبجديات العالم',
    act8Title: 'دستور غير قابل للخرق فوق كل نموذج، متجذر في التعدد اللغوي.',
    act8Body:
      'عشر قواعد آلية تُطبَّق قبل أي استنتاج عصبي — من حماية القُصّر في أقل من 1 مللي ثانية إلى بروتوكول المساندة الإنسانية الهادئ.',

    act9Eyebrow: 'الفصل التاسع · ميثاق الصدق والبدء السريع للمطورين',
    act9Title: 'تحقق من كل ادعاء بنفسك عبر cargo test.',
    act9Body:
      'بلا قوائم انتظار مصطنعة: استنسخ المستودع المرخص بـ Apache-2.0 وشغّل اختبارات المطابقة بنفسك.',
    act9CopyCmd: 'نسخ',
    act9Copied: 'تم النسخ',
  },
  kora: {
    title: 'الوهج تحت قوس الـ 21 خيطًا',
    subtitle: 'آلة الحضور الحي (brand.rs · 21 وتر كورا · توليد Karplus-Strong)',
    stateIdle: 'سكون (0 fps)',
    stateListening: 'استماع',
    stateThinking: 'تفكير',
    stateSpeaking: 'نطق (21 وترًا)',
    stateDecision: 'قرار الجمر',
    stateEmergency: 'مساندة هادئة',
    bargeInBtn: 'مقاطعة فورية للنطق (< 80 ms)',
    bargeInStatus: 'تمت المقاطعة خلال 42 مللي ثانية وانطوت الموجة فورًا.',
    pluckHint: 'مرّر المؤشر أو انقر على أوتار الكورا الـ 21 (فعّل صوت 48 kHz بالأعلى لسماعها).',
    fpsZeroBadge: 'حلقة الرسوم متوقفة · 0.0 fps',
    fpsActiveBadge: 'موجة نشطة · 21 وترًا',
  },
  spotlight: {
    placeholder: 'اكتب أمرًا سياديًا (:status, :models-honest, :powers, :budget) أو انتقل إلى صفحة…',
    hintBar: 'Enter للتنفيذ · الأسهم للاختيار · Esc للإغلاق',
    closeEsc: 'Esc',
    emptyPrompt: 'أوامر سيادية فورية:',
    runBtn: 'تنفيذ',
  },
  actionLab: {
    scenarioInjectionTab: '1. بريد مفخخ (Untrusted)',
    scenarioTransferTab: '2. تحويل 25€ (غير قابل للتراجع)',
    scenarioArchiveTab: '3. أرشفة (قابل للتراجع)',
    untrustedBadge: '[خارجي · Untrusted]',
    trustedBadge: '[نية بشرية موثّقة]',
    costPill: 'التكلفة: 0.00 € (محلي)',
    dataPill: 'البيانات: 0 بايت خارجي',
    revPillIrreversible: 'غير قابل للتراجع · ضغطة 600 ms',
    revPillReversible: 'قابل للتراجع بنقرة واحدة',
    holdButtonLabel: 'اضغط مطولًا 600 مللي ثانية للاعتماد',
    holdHoldingLabel: 'جارٍ الضغط…',
    holdArmedLabel: 'الخطوة 1/2 جاهزة — اضغط مجددًا للتأكيد',
    holdConfirmedLabel: 'تم الختم والتوقيع بـ Ed25519',
    holdCancelledHint: 'تم الإفلات قبل 600 مللي ثانية: لم يُنفذ أي أثر.',
    accessibleModeToggle: 'وضع لوحة المفاتيح الميسّر (ضغطتان دون مؤقّت)',
    blockedByRule01:
      'حظر صارم · RULE-01: المصدر خارجي TrustLevel::Untrusted. تم تعطيل الاعتماد بنيويًا وتسجيل الحادثة في السجل.',
    executeReversibleBtn: 'تنفيذ الأرشفة القابلة للتراجع',
    auditHeading: 'سجل التدقيق التشفيري الحي',
    chainValidBadge: 'سلسلة SHA-256 والتواقيع سليمة',
    chainBrokenBadge: 'تنبيه: تم رصد تلاعب تشفيري في السلسلة!',
    tamperBtn: 'محاكاة تلاعب ببايت واحد',
    restoreChainBtn: 'استعادة السجل السليم',
    whyBtn: 'لماذا؟ (3 مستويات)',
    undoBtn: 'تراجع',
    whyLevel1: 'المستوى 1 · شرح واضح في جملة واحدة',
    whyLevel2: 'المستوى 2 · حقائق الذاكرة SPO المستند إليها',
    whyLevel3: 'المستوى 3 · الإثبات التقني ورمز Biscuit والافتراض المضاد',
    markFactFalseBtn: 'هذه الحقيقة غير صحيحة',
    factDisputedBadge: 'أبطلته أنت',
  },
  memoryLab: {
    partitionPersonal: 'القسم الشخصي',
    partitionPro: 'القسم المهني',
    partitionShared: 'قسم المنزل المشترك',
    timeSliderLabel: 'مرور الزمن (تلاشي الذاكرة والمسح الإلزامي خلال 5 دقائق)',
    timeNow: 'الآن (0 دقيقة)',
    time5m: '5 دقائق (مسح AI Act)',
    time30d: '30 يومًا',
    time2y: 'سنتان',
    aiActNotice:
      'امتثال قانون الذكاء الاصطناعي الأوروبي والقاعدة 8: عند مرور 5 دقائق تُمحى الإشارات الانفعالية (cognition:*) نهائيًا دون أثر.',
    exportJsonBtn: 'تنزيل hakille-graph-export JSON v1',
    digitalTwilightTitle: 'ميثاق «الشفق الرقمي»',
    digitalTwilightBody:
      'حدّد بتوجيه موقّع مصير ذاكرتك بعد انقطاع النشاط الطويل: إتلاف تشفيري كامل أو نقل انتقائي لوصيّ معتمد.',
  },
  fabricLab: {
    layersTab: 'مقطع الطبقات الست',
    cratesTab: 'مستكشف الـ 41 حزمة',
    system2Tab: 'محاكي Biscuit لمدة 300 ثانية',
    searchCratesPlaceholder: 'ابحث باسم الحزمة أو التقنية (wgpu, redb, biscuit, whisper)…',
    allLayersFilter: 'كل الطبقات (41)',
    localSystem1Title: 'النظام 1 · استدلال محلي بلا إنترنت',
    remoteSystem2Title: 'النظام 2 · استعانة خارجية بعقد Biscuit لمدة 300 ثانية',
    issueBiscuitBtn: 'إصدار رمز Biscuit لمدة 300 ثانية',
    simulateStreamBtn: 'محاكاة طلب (+1.8 KB / +0.004 €)',
    revokeTokenBtn: 'إلغاء الرمز فورًا',
  },
  ambientLab: {
    channelHeading: 'محدد القناة متعدد الوسائط',
    noiseLabel: 'مستوى الضجيج المحيط',
    meetingToggle: 'في اجتماع (التقويم / الميكروفون)',
    thirdPartyToggle: 'وجود طرف ثالث في الغرفة',
    headphonesToggle: 'سماعات خاصة متصلة',
    silenceBudgetHeading: 'ميزانية الصمت (3 تنبيهات / يوم)',
    triggerNudgeBtn: 'إرسال اقتراح استباقي',
    dismissTopicBtn: 'رفض الموضوع (صمت 14 يومًا)',
    resetDayBtn: 'يوم جديد',
    autonomyHeading: 'سلّم الاستقلالية حسب المجال (0 → 4)',
    validate20Btn: '+5 إجراءات ناجحة',
    promoteCrankBtn: 'قبول الترقية للدرجة التالية',
    humanUndoBtn: 'إلغاء إجراء (خفض فوري -1 درجة)',
  },
  inspector: {
    title: 'فاحص «انعدام التسريب» الحي',
    subtitle: 'فحص مباشر يعمل داخل متصفحك على هذه الصفحة الآن.',
    cookiesLabel: 'ملفات الارتباط (document.cookie)',
    thirdPartyReqLabel: 'طلبات النطاقات الخارجية (Performance API)',
    fontShaLabel: 'الخط المحلي InterVariable.ttf (SIL OFL 1.1)',
    storageKeysLabel: 'مفاتيح التفضيلات المحلية (localStorage)',
    rescanBtn: 'إعادة الفحص الحي',
    wipeLocalBtn: 'مسح التفضيلات المحلية بنقرة واحدة',
    wipedConfirm: 'تم مسح التفضيلات المحلية واستعادة الإعدادات الافتراضية.',
  },
  common: {
    readFullDoc: 'استكشف المواصفات الكاملة',
    verifiedInRepo: 'موثّق في المستودع عند HEAD',
    preAlphaWarningTitle: 'شفافية إصدار ما قبل ألفا v0.1.0',
    preAlphaWarningBody:
      'يعلن هاكيلي بوضوح تام ما يعمل فعليًا اليوم مقابل ما يزال قيد التطوير.',
    plainSummaryTitle: 'الملخص الواضح (في 30 ثانية)',
    lastUpdated: 'آخر تحديث',
    filterAll: 'الكل',
  },
};
