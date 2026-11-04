package dev.hakille.mobile

import android.os.Bundle
import androidx.activity.ComponentActivity
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
        enableEdgeToEdge()
        setContent {
            var forceRtl by rememberSaveable { mutableStateOf(false) }
            var appearance by rememberSaveable { mutableStateOf(Appearance.DARK) }
            val systemDirection = LocalLayoutDirection.current
            HakilleTheme(appearance) {
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
    val leadingItems = (if (restoring) 1 else 0) + (if (persistenceError.isNotEmpty()) 1 else 0) +
        (if (uncertain.isNotEmpty()) 1 else 0) + uncertain.size + (if (error.isNotEmpty()) 1 else 0) +
        (if (requestPending) 1 else 0) + (if (lastOutcome != null) 1 else 0) + notices.size + snapshot.rows("cards").size
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
                TextButton(onClick = { navigate(AppDestination.HUB); hubSection = 1 }) {
                    Text(stringResource(headerStateLabel), style = MaterialTheme.typography.labelMedium)
                }
            }
        },
        bottomBar = { HakilleNavigation(destination, ::navigate) }) { padding ->
        // La largeur est bornée sur tablette ; le texte reste libre de grandir et de défiler.
        Box(Modifier.fillMaxSize().padding(padding).consumeWindowInsets(padding).imePadding(), contentAlignment = Alignment.TopCenter) {
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
                if (error.isNotEmpty()) item { StateNotice(error, error = true) }
                if (requestPending) item { QuietPanel {
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                    Text(stringResource(R.string.waiting), style = MaterialTheme.typography.bodySmall)
                    TextButton(onClick = model::cancelRequest) { Text(stringResource(R.string.cancel_request)) }
                } }
                if (lastOutcome != null) item { StateNotice(lastOutcome.text("message"), error = lastOutcome.text("state") == "failed") }
                items(notices) { text -> StateNotice(text) }
                items(snapshot.rows("cards"), key = { "card-${it.text("id")}" }) { card -> DecisionCard(card, snapshot.objectOrNull("approval"), busy, model) }
                when (destination) {
                    AppDestination.VOICE -> {
                        item("voice-presence") { DictationControls(model, enabled = !busy && snapshot.rows("cards").isEmpty(), onWrite = { scope.launch { listState.animateScrollToItem(leadingItems + 1); withFrameNanos { }; if (destination == AppDestination.VOICE) runCatching { draftFocus.requestFocus() } } },
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
                        item { QuietPanel {
                            OutlinedTextField(search, { search = it }, label = { Text(stringResource(R.string.search)) }, modifier = Modifier.fillMaxWidth())
                            Button(onClick = { model.dispatch("load_memory") { put("query", search.ifBlank { null }); put("limit", 50) } }, enabled = hasHub && !busy) { Text(stringResource(R.string.search)) }
                        } }
                        if (snapshot.rows("memory").isEmpty()) item { StateNotice(stringResource(R.string.ui_memory_empty)) }
                        items(snapshot.rows("memory")) { row -> QuietPanel {
                            SelectionContainer { Text(row.text("text")) }
                            Text("${row.text("source")} · ${row["tags"] ?: ""}", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        } }
                        item { QuietPanel {
                            Text(stringResource(R.string.remember), style = MaterialTheme.typography.titleMedium)
                            OutlinedTextField(memoryText, { memoryText = it }, label = { Text(stringResource(R.string.memory_text)) }, modifier = Modifier.fillMaxWidth())
                            Button(onClick = { model.dispatch("remember") { put("text", memoryText) } }, enabled = hasHub && !busy && memoryText.isNotBlank()) { Text(stringResource(R.string.remember)) }
                        } }
                    }
                    AppDestination.ACTIVITY -> {
                        item { SectionHeading(stringResource(R.string.ui_activity), stringResource(R.string.ui_activity_hint)) }
                        item { FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            listOf(R.string.journal, R.string.agents).forEachIndexed { index, label -> FilterChip(selected = activitySection == index, onClick = { activitySection = index }, label = { Text(stringResource(label)) }) }
                        } }
                        item { Button(onClick = { model.dispatch(if (activitySection == 0) "load_journal" else "load_agents") { if (activitySection == 0) put("limit", 50) } }, enabled = hasHub && !busy) { Text(stringResource(R.string.refresh)) } }
                        if (activitySection == 0) {
                            if (snapshot.rows("journal").isEmpty()) item { StateNotice(stringResource(R.string.ui_journal_empty)) }
                            items(snapshot.rows("journal")) { row -> QuietPanel {
                                Text(row.text("timestamp"), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                Text(row.text("action"), style = MaterialTheme.typography.titleMedium)
                                SelectionContainer { Text(row.text("summary")) }
                            } }
                            if (snapshot.rows("outcomes").isNotEmpty()) item { Text(stringResource(R.string.outcomes), style = MaterialTheme.typography.titleMedium) }
                            items(snapshot.rows("outcomes")) { outcome -> QuietPanel {
                                Text(outcome.text("state"), style = MaterialTheme.typography.labelMedium)
                                SelectionContainer { Text(outcome.text("message")) }
                            } }
                        } else {
                            if (snapshot.rows("agents").isEmpty()) item { StateNotice(stringResource(R.string.ui_agents_empty)) }
                            items(snapshot.rows("agents"), key = { "agent-${it.text("name")}" }) { agent -> AgentCard(agent, !busy && hasHub, model) }
                        }
                    }
                    AppDestination.HUB -> {
                        item { SectionHeading(stringResource(R.string.ui_hub), stringResource(R.string.ui_hub_hint)) }
                        item { FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            listOf(R.string.status, R.string.connection, R.string.ui_device).forEachIndexed { index, label ->
                                FilterChip(selected = hubSection == index, onClick = { hubSection = index }, label = { Text(stringResource(label)) }, modifier = Modifier.testTag(when(index) { 1 -> "destination_connection"; 2 -> "destination_capabilities"; else -> "destination_status" }))
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
                        }
                    }
                }
                if (!hasHub && destination != AppDestination.HUB) item {
                    TextButton(onClick = { navigate(AppDestination.HUB); hubSection = 1 }) { Text(stringResource(R.string.ui_connect_hub)) }
                }
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
    val id = card.getValue("id") // Preserve all 64 bits; never convert protocol IDs to floating point.
    val armed = approval?.get("card_id") == id
    val accent = tokenColor(if (LocalHakilleDark.current) HakilleTokens.colorBraiseDark else HakilleTokens.colorBraiseLight)
    val onAccent = tokenColor(if (LocalHakilleDark.current) HakilleTokens.colorSurBraiseDark else HakilleTokens.colorSurBraiseLight)
    val description = stringResource(R.string.approval_a11y)
    val currentDispatch by rememberUpdatedState<(String) -> Unit>({ action -> model.dispatch(action) { put("card_id", id) } })
    Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.decision), color = accent, style = MaterialTheme.typography.titleMedium)
        Text("${card.text("verb")} ${card.text("object")}", style = MaterialTheme.typography.titleLarge)
        Text("${stringResource(R.string.reason)} : ${card.text("reason")}")
        Text("${stringResource(R.string.cost)} : ${CardPresentation.money(card["cost"])}")
        Text("${stringResource(R.string.data_exit)} : ${CardPresentation.destination(card["data_exit"], stringResource(R.string.data_local), stringResource(R.string.data_cloud), stringResource(R.string.metadata_unknown))}")
        Text("${stringResource(R.string.provenance)} : ${CardPresentation.provenance(card["provenance"], stringResource(R.string.external_origin), stringResource(R.string.verified_skill), stringResource(R.string.metadata_unknown))}")
        Text(stringResource(if (card["reversible"]?.jsonPrimitive?.booleanOrNull == true) R.string.reversible else R.string.irreversible))
        if (!busy) {
            Box(Modifier.fillMaxWidth().heightIn(min = 56.dp).background(accent, MaterialTheme.shapes.medium)
                .semantics {
                    role = Role.Button; contentDescription = description
                    onClick { currentDispatch(if (armed) "approve" else "begin_approval"); true }
                }.pointerInput(id) {
                    detectTapGestures(onPress = {
                        currentDispatch("begin_approval")
                        var ended = false
                        try {
                            val released = tryAwaitRelease()
                            if (released) currentDispatch("approve") else model.dispatch("cancel_approval")
                            ended = true
                        } finally { if (!ended) model.dispatch("cancel_approval") }
                    })
                }.padding(16.dp), contentAlignment = Alignment.Center) {
                Text(stringResource(if (armed) R.string.confirm_ready else R.string.hold_approve), color = onAccent)
            }
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { currentDispatch("reject") }) { Text(stringResource(R.string.reject)) }
                if (armed) TextButton(onClick = { model.dispatch("cancel_approval") }) { Text(stringResource(R.string.cancel_approval)) }
            }
        }
    } }
}
