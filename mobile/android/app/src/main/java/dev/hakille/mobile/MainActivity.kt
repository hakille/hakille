package dev.hakille.mobile

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import kotlinx.coroutines.launch
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import hakille.tokens.HakilleTokens
import kotlinx.serialization.json.*

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.auto(
                lightScrim = android.graphics.Color.parseColor(HakilleTokens.colorSurface1LightHex),
                darkScrim = android.graphics.Color.parseColor(HakilleTokens.colorSurface1DarkHex),
                detectDarkMode = { true },
            ),
        )
        setContent {
            var forceRtl by rememberSaveable { mutableStateOf(false) }
            var appearance by rememberSaveable { mutableStateOf(Appearance.DARK) }
            val systemDirection = LocalLayoutDirection.current
            HakilleTheme(appearance) {
                val dark = LocalHakilleDark.current
                // Les icônes système suivent le thème affiché, pas uniquement celui du téléphone.
                SideEffect {
                    this@MainActivity.enableEdgeToEdge(
                        statusBarStyle = if (dark) SystemBarStyle.dark(android.graphics.Color.TRANSPARENT)
                            else SystemBarStyle.light(android.graphics.Color.TRANSPARENT, android.graphics.Color.TRANSPARENT),
                        // AUTO conserve la protection de contraste de la navigation à trois boutons.
                        navigationBarStyle = SystemBarStyle.auto(
                            lightScrim = android.graphics.Color.parseColor(HakilleTokens.colorSurface1LightHex),
                            darkScrim = android.graphics.Color.parseColor(HakilleTokens.colorSurface1DarkHex),
                            detectDarkMode = { dark },
                        ),
                    )
                }
                CompositionLocalProvider(LocalLayoutDirection provides if (forceRtl) LayoutDirection.Rtl else systemDirection) {
                    MobileScreen(forceRtl, { forceRtl = it }, appearance, { appearance = it })
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class, ExperimentalComposeUiApi::class)
@Composable private fun MobileScreen(forceRtl: Boolean, changeRtl: (Boolean) -> Unit, appearance: Appearance,
    changeAppearance: (Appearance) -> Unit, model: MobileModel = viewModel()) {
    val snapshot by model.snapshot.collectAsStateWithLifecycle()
    val error by model.error.collectAsStateWithLifecycle()
    val restoring by model.restoring.collectAsStateWithLifecycle()
    val localStateAvailable by model.localStateAvailable.collectAsStateWithLifecycle()
    val persistenceError by model.persistenceError.collectAsStateWithLifecycle()
    val savedEndpoint by model.savedEndpoint.collectAsStateWithLifecycle()
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, event ->
            if (event == Lifecycle.Event.ON_RESUME) model.foreground(true)
            if (event == Lifecycle.Event.ON_PAUSE) model.foreground(false)
        }
        lifecycle.addObserver(observer)
        onDispose { lifecycle.removeObserver(observer); model.foreground(false) }
    }
    var destination by rememberSaveable { mutableStateOf(AppDestination.VOICE) }
    var activitySection by rememberSaveable { mutableIntStateOf(0) }
    var hubSection by rememberSaveable { mutableIntStateOf(0) }
    var endpoint by remember { mutableStateOf("") }
    var token by remember { mutableStateOf("") }
    var search by rememberSaveable { mutableStateOf("") }
    var memoryText by rememberSaveable { mutableStateOf("") }
    var showDistress by rememberSaveable { mutableStateOf(false) }
    val listState = rememberLazyListState()
    val scope = rememberCoroutineScope()
    val draftFocus = remember { FocusRequester() }
    LaunchedEffect(destination, hubSection, activitySection) { listState.scrollToItem(0) }
    val requestPending = snapshot.objectOrNull("pending") != null
    val busy = requestPending || !localStateAvailable
    val hasHub = snapshot.text("hub_id").isNotEmpty()
    val uncertain = snapshot.rows("uncertain_operations").filter { it.text("phase") == "unknown" }
    val lastOutcome = snapshot.rows("outcomes").lastOrNull()?.takeIf { it.text("state") != "confirmed" || it.text("kind") in setOf("remember", "run_agent", "approve", "reject") }
    val notices = (snapshot["notifications"] as? JsonArray)?.map { it.jsonPrimitive.content }.orEmpty()
    val visibleCards = dev.hakille.mobile.ui.ActionCardHelper.filterSingleDecision(snapshot.rows("cards"))
    val leadingItems = (if (restoring) 1 else 0) + (if (persistenceError.isNotEmpty()) 1 else 0) +
        (if (uncertain.isNotEmpty()) 1 else 0) + uncertain.size + (if (error.isNotEmpty()) 1 else 0) +
        (if (requestPending) 1 else 0) + (if (lastOutcome != null) 1 else 0) + notices.size + visibleCards.size
    val stateLabel = when (snapshot.text("connection")) {
        "connected" -> R.string.connected; "connecting" -> R.string.connecting; "unknown" -> R.string.unknown; else -> R.string.disconnected
    }
    val headerStateLabel = when (snapshot.text("connection")) {
        "connected" -> R.string.ui_hub_online; "connecting" -> R.string.ui_hub_connecting
        "unknown" -> R.string.ui_hub_check; else -> R.string.ui_hub_offline
    }
    fun navigate(next: AppDestination) {
        if (next != destination) { model.dictation.cancel(); model.stopVoiceTurn(); model.dispatch("cancel_approval") }
        destination = next
    }
    Scaffold(modifier = Modifier.semantics { testTagsAsResourceId = true },
        containerColor = MaterialTheme.colorScheme.background,
        contentWindowInsets = WindowInsets.safeDrawing,
        topBar = {
            Row(Modifier.fillMaxWidth().statusBarsPadding().padding(horizontal = 24.dp, vertical = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("hakille", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
                TextButton(onClick = { showDistress = true }) {
                    Text("+ Urgence", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                TextButton(onClick = { navigate(AppDestination.HUB); hubSection = 1 }) {
                    Text(stringResource(headerStateLabel), style = MaterialTheme.typography.labelMedium)
                }
            }
        },
        bottomBar = { HakilleNavigation(destination, ::navigate) }) { padding ->
        // La largeur est bornée sur tablette ; le texte reste libre de grandir et de défiler.
        Box(
            Modifier.fillMaxSize()
                .padding(padding)
                .consumeWindowInsets(padding)
                .imePadding()
                .pointerInput(Unit) {
                    detectTapGestures { model.stopVoicePlayback() }
                },
            contentAlignment = Alignment.TopCenter
        ) {
            LazyColumn(Modifier.widthIn(max = 720.dp).fillMaxSize().testTag("screen_${destination.name.lowercase()}"), state = listState, contentPadding = PaddingValues(start = 24.dp, end = 24.dp, top = 16.dp, bottom = 32.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                if (restoring) item { StateNotice(stringResource(R.string.draft_restoring)) }
                if (persistenceError.isNotEmpty()) item { StateNotice(persistenceError, error = true) }
                if (uncertain.isNotEmpty()) item { StateNotice(stringResource(R.string.recovery_explanation)) }
                items(uncertain, key = { "uncertain-${it.text("request_id")}" }) { operation ->
                    QuietPanel {
                        Text("${operation.text("kind")} · #${operation.text("request_id")}", style = MaterialTheme.typography.labelMedium)
                        TextButton(onClick = { model.dispatch("acknowledge_unknown") { put("request_id", operation.getValue("request_id")) } }, enabled = !busy) { Text(stringResource(R.string.recovery_acknowledge)) }
                    }
                }
                if (error.isNotEmpty()) item {
                    dev.hakille.mobile.ui.HonestErrorComponent(
                        errorState = dev.hakille.mobile.ui.ActionErrorState(
                            done = "Brouillon sauvegardé localement",
                            notDone = "Requête non transmise au hub",
                            technicalDetail = error
                        ),
                        onRetry = {
                            if (snapshot.text("draft").isNotBlank()) model.dispatch("submit")
                            else model.dispatch("refresh_status")
                        },
                        onKeepDraft = { /* brouillon conservé */ }
                    )
                }
                if (requestPending) item {
                    dev.hakille.mobile.ui.HonestWaitComponent(
                        startTimeMs = model.requestStartTimeMs.collectAsState().value,
                        onCancel = model::cancelRequest
                    )
                }
                if (lastOutcome != null) item {
                    if (lastOutcome.text("state") == "failed") {
                        dev.hakille.mobile.ui.HonestErrorComponent(
                            errorState = dev.hakille.mobile.ui.ActionErrorState(
                                done = "Opération tracée localement",
                                notDone = "Échec du traitement sur le hub",
                                technicalDetail = lastOutcome.text("message")
                            ),
                            onRetry = { model.dispatch("refresh_status") },
                            onKeepDraft = { /* brouillon conservé */ }
                        )
                    } else {
                        StateNotice(lastOutcome.text("message"), error = false)
                    }
                }
                val privacyGuard = snapshot.objectOrNull("privacy_guard")
                if (privacyGuard != null) item("privacy-guard") {
                    dev.hakille.mobile.ui.PrivacyGuardBanner(
                        event = privacyGuard,
                        onDismiss = model::dismissPrivacyGuard
                    )
                }
                val ghostAction = snapshot.objectOrNull("ghost_action")
                if (ghostAction != null) item("ghost-action") {
                    dev.hakille.mobile.ui.GhostActionComponent(
                        ghost = ghostAction,
                        onAccept = model::ghostActionAccept,
                        onDismiss = model::ghostActionDismiss
                    )
                }
                items(notices) { text ->
                    if (text.startsWith("Conseil bien-être :")) {
                        dev.hakille.mobile.ui.FatigueAlertComponent(
                            recommendation = text.removePrefix("Conseil bien-être :").trim(),
                            onDismiss = { /* dismiss */ }
                        )
                    } else {
                        StateNotice(text)
                    }
                }
                val visibleCards = dev.hakille.mobile.ui.ActionCardHelper.filterSingleDecision(snapshot.rows("cards"))
                items(visibleCards, key = { "card-${it.text("id")}" }) { card -> DecisionCard(card, snapshot.objectOrNull("approval"), busy, model) }
                when (destination) {
                    AppDestination.VOICE -> {
                        item("voice-presence") { DictationControls(model, enabled = !busy && visibleCards.isEmpty(), onWrite = { scope.launch { listState.animateScrollToItem(leadingItems + 1); withFrameNanos { }; if (destination == AppDestination.VOICE) runCatching { draftFocus.requestFocus() } } },
                            onJournal = { navigate(AppDestination.ACTIVITY); activitySection = 0 }, onDevice = { navigate(AppDestination.HUB); hubSection = 2 }) }
                        item("draft") { QuietPanel {
                            Text(stringResource(R.string.ui_write_title), style = MaterialTheme.typography.titleMedium, modifier = Modifier.semantics { heading() })
                            OutlinedTextField(value = snapshot.text("draft"), onValueChange = { text -> model.dispatch("set_draft") { put("text", text) } },
                                label = { Text(stringResource(R.string.message)) }, modifier = Modifier.fillMaxWidth().testTag("draft_input").focusRequester(draftFocus), enabled = !busy, minLines = 2)
                            Button(onClick = { model.dispatch("submit") }, enabled = hasHub && !busy && snapshot.text("draft").isNotBlank(), modifier = Modifier.align(Alignment.End)) { Text(stringResource(R.string.send)) }
                        } }
                        if (snapshot.rows("chat").isNotEmpty()) item { Text(stringResource(R.string.ui_recent_exchange), style = MaterialTheme.typography.titleMedium, modifier = Modifier.semantics { heading() }) }
                        items(snapshot.rows("chat")) { row -> QuietPanel {
                            Text(stringResource(if (row.text("role") == "user") R.string.you else R.string.assistant), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            SelectionContainer { Text(row.text("text"), style = MaterialTheme.typography.bodyLarge) }
                        } }
                        if (snapshot["history_trimmed"]?.jsonPrimitive?.booleanOrNull == true) item { StateNotice(stringResource(R.string.history_trimmed)) }
                    }
                    AppDestination.MEMORY -> {
                        item { SectionHeading(stringResource(R.string.memory), stringResource(R.string.ui_memory_hint)) }
                        item {
                            dev.hakille.mobile.ui.MemoryGraphComponent(
                                memoryRows = snapshot.rows("memory"),
                                tombstones = (snapshot["tombstones"] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }.orEmpty(),
                                busy = busy,
                                onSearch = { model.loadMemory(it.ifBlank { null }) },
                                onRemember = { model.rememberFact(it) },
                                onForgetFact = { model.forgetFact(it) }
                            )
                        }
                    }
                    AppDestination.ACTIVITY -> {
                        item { SectionHeading(stringResource(R.string.ui_activity), stringResource(R.string.ui_activity_hint)) }
                        item { FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            listOf(R.string.journal, R.string.agents).forEachIndexed { index, label -> FilterChip(selected = activitySection == index, onClick = { activitySection = index }, label = { Text(stringResource(label)) }) }
                        } }
                        if (activitySection == 0) {
                            item {
                                dev.hakille.mobile.ui.AuditJournalComponent(
                                    entries = snapshot.rows("journal"),
                                    busy = busy,
                                    onRefresh = { model.loadJournal() },
                                    onUndo = { model.undoAction(it) },
                                    onRequestWhy = { model.requestWhy(it, "journal") }
                                )
                            }
                            if (snapshot.rows("outcomes").isNotEmpty()) item { Text(stringResource(R.string.outcomes), style = MaterialTheme.typography.titleMedium) }
                            items(snapshot.rows("outcomes")) { outcome -> QuietPanel {
                                Text(outcome.text("state"), style = MaterialTheme.typography.labelMedium)
                                SelectionContainer { Text(outcome.text("message")) }
                            } }
                        } else {
                            item {
                                dev.hakille.mobile.ui.AgentCatalogComponent(
                                    agents = snapshot.rows("agents"),
                                    lastAgentRun = snapshot.objectOrNull("last_agent_run"),
                                    busy = busy,
                                    onRefresh = { model.loadAgents() },
                                    onRunAgent = { name, input -> model.runAgent(name, input) }
                                )
                            }
                        }
                    }
                    AppDestination.HUB -> {
                        item { SectionHeading(stringResource(R.string.ui_hub), stringResource(R.string.ui_hub_hint)) }
                        val hubTabs = listOf(stringResource(R.string.status), stringResource(R.string.connection), stringResource(R.string.ui_device), "Autonomie")
                        item { FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            hubTabs.forEachIndexed { index, label ->
                                FilterChip(selected = hubSection == index, onClick = { hubSection = index }, label = { Text(label) }, modifier = Modifier.testTag(when(index) { 1 -> "destination_connection"; 2 -> "destination_capabilities"; 3 -> "destination_autonomy"; else -> "destination_status" }))
                            }
                        } }
                        when (hubSection) {
                            0 -> {
                                item { QuietPanel {
                                    Text(stringResource(stateLabel), style = MaterialTheme.typography.titleMedium)
                                    JsonDetails(snapshot.objectOrNull("status"))
                                    Button(onClick = { model.dispatch("refresh_status") }, enabled = hasHub && !busy) { Text(stringResource(R.string.refresh)) }
                                } }
                                item { QuietPanel {
                                    Text(stringResource(R.string.ui_appearance), style = MaterialTheme.typography.titleMedium)
                                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                        Appearance.entries.forEach { option -> FilterChip(selected = appearance == option, onClick = { changeAppearance(option) }, label = { Text(stringResource(when (option) { Appearance.DARK -> R.string.ui_dark; Appearance.LIGHT -> R.string.ui_light; Appearance.SYSTEM -> R.string.ui_system })) }) }
                                    }
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        val label = stringResource(R.string.rtl_layout)
                                        Text(label, Modifier.weight(1f)); Switch(checked = forceRtl, onCheckedChange = changeRtl, modifier = Modifier.semantics { contentDescription = label })
                                    }
                                } }
                            }
                            1 -> item { QuietPanel {
                                Text(stringResource(R.string.connection_help), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                OutlinedTextField(endpoint, { endpoint = it; token = "" }, label = { Text(stringResource(R.string.endpoint)) }, singleLine = true, modifier = Modifier.fillMaxWidth().testTag("connection_endpoint"))
                                OutlinedTextField(token, { token = it }, label = { Text(stringResource(R.string.token)) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false), modifier = Modifier.fillMaxWidth().testTag("connection_token"))
                                Button(onClick = { model.connect(endpoint, token); token = "" }, enabled = endpoint.isNotBlank() && token.isNotBlank() && !busy) { Text(stringResource(R.string.connect)) }
                                if (savedEndpoint.isNotEmpty()) {
                                    Text(savedEndpoint, style = MaterialTheme.typography.bodySmall)
                                    OutlinedButton(onClick = { model.connect("", "", useSaved = true) }, enabled = !busy) { Text(stringResource(R.string.saved_hub)) }
                                }
                                TextButton(onClick = { model.disconnect(); token = "" }, enabled = !restoring) { Text(stringResource(R.string.forget)) }
                            } }
                            2 -> item { Column(Modifier.testTag("screen_capabilities")) { DeviceCapabilitiesPanel(model) } }
                            3 -> {
                                item("guest-pass") {
                                    dev.hakille.mobile.ui.GuestPassPanel(
                                        guestPass = snapshot.objectOrNull("guest_pass"),
                                        busy = busy,
                                        onStartGuestPass = { name, power, dur ->
                                            model.startGuestPass(name, power, dur)
                                        },
                                        onRevokeGuestPass = model::revokeGuestPass
                                    )
                                }
                                item("autonomy-ladder") {
                                    dev.hakille.mobile.ui.AutonomyLadderComponent(
                                        domainAutonomies = snapshot.rows("domain_autonomies"),
                                        onSaveCrank = { domain, crank, maxCost, maxActions, daysValid ->
                                            model.setAutonomyCrank(domain, crank, maxCost, maxActions, daysValid)
                                        }
                                    )
                                }
                            }
                        }
                    }
                }
                if (!hasHub && destination != AppDestination.HUB) item {
                    TextButton(onClick = { navigate(AppDestination.HUB); hubSection = 1 }) { Text(stringResource(R.string.ui_connect_hub)) }
                }
            }
            val whySheet = snapshot.objectOrNull("why_sheet")
            if (whySheet != null) {
                dev.hakille.mobile.ui.WhySheetComponent(
                    whySheet = whySheet,
                    onDismiss = { model.closeWhy() },
                    onDisputeFact = { factId -> model.forgetFact(factId) }
                )
            }
            if (snapshot["privacy_veil_active"]?.jsonPrimitive?.booleanOrNull == true) {
                dev.hakille.mobile.ui.PrivacyGuardVeilOverlay(
                    event = snapshot.objectOrNull("privacy_guard"),
                    onDismiss = model::dismissPrivacyGuard,
                    modifier = Modifier.fillMaxSize()
                )
            }
            if (showDistress || snapshot["distress"]?.jsonPrimitive?.booleanOrNull == true) {
                dev.hakille.mobile.ui.CalmDistressMode(
                    trustedContact = snapshot.text("trusted_contact").ifBlank { null },
                    onDismiss = { showDistress = false },
                    modifier = Modifier.fillMaxSize()
                )
            }
        }
    }
}
@Composable private fun JsonDetails(value: JsonObject?) {
    if (value == null) Text(stringResource(R.string.empty))
    else value.forEach { (key, content) -> SelectionContainer { Text("$key : ${if (content is JsonPrimitive) content.content else content.toString()}") } }
}

@Composable private fun AgentCard(agent: JsonObject, enabled: Boolean, model: MobileModel) {
    var input by remember(agent.text("name")) { mutableStateOf("{}") }
    Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(agent.text("name"), style = MaterialTheme.typography.titleMedium)
        JsonDetails(agent)
        if (agent["artifact_present"]?.jsonPrimitive?.booleanOrNull == true) {
            OutlinedTextField(input, { input = it }, label = { Text(stringResource(R.string.agent_input)) }, modifier = Modifier.fillMaxWidth())
            Button(onClick = { model.dispatch("run_agent") { put("name", agent.text("name")); put("input_json", input) } }, enabled = enabled) { Text(stringResource(R.string.run_agent)) }
        } else Text(stringResource(R.string.agent_unavailable))
    } }
}

@Composable private fun DecisionCard(card: JsonObject, approval: JsonObject?, busy: Boolean, model: MobileModel) {
    dev.hakille.mobile.ui.ActionCardComponent(
        card = card,
        approval = approval,
        busy = busy,
        onApprove = { id -> model.dispatch("approve") { put("card_id", id) } },
        onReject = { id -> model.dispatch("reject") { put("card_id", id) } },
        onEdit = { id -> model.dispatch("edit_card") { put("card_id", id) } },
        onRequestWhy = { id -> model.requestWhy(id.toString(), "card") },
        onCancelApproval = { model.dispatch("cancel_approval") }
    )
}
