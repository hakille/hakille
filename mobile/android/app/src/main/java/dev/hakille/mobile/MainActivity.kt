package dev.hakille.mobile

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.*
import androidx.compose.ui.text.input.PasswordVisualTransformation
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
            val systemDirection = LocalLayoutDirection.current
            HakilleTheme { CompositionLocalProvider(LocalLayoutDirection provides if (forceRtl) LayoutDirection.Rtl else systemDirection) { MobileScreen(forceRtl = forceRtl, changeRtl = { forceRtl = it }) } }
        }
    }
}

private fun tokenColor(value: String) = Color(android.graphics.Color.parseColor(value))
@Composable private fun HakilleTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val background = tokenColor(if (dark) HakilleTokens.colorSurface0Dark else HakilleTokens.colorSurface0Light)
    val surface = tokenColor(if (dark) HakilleTokens.colorSurface1Dark else HakilleTokens.colorSurface1Light)
    val foreground = tokenColor(if (dark) HakilleTokens.colorTexte1Dark else HakilleTokens.colorTexte1Light)
    val colors = if (dark) darkColorScheme(background = background, surface = surface, onSurface = foreground, primary = foreground)
        else lightColorScheme(background = background, surface = surface, onSurface = foreground, primary = foreground)
    MaterialTheme(colorScheme = colors, content = content)
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable private fun MobileScreen(forceRtl: Boolean, changeRtl: (Boolean) -> Unit, model: MobileModel = viewModel()) {
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
    var tab by remember { mutableIntStateOf(0) }
    var endpoint by remember { mutableStateOf("") }
    var token by remember { mutableStateOf("") }
    var search by remember { mutableStateOf("") }
    var memoryText by remember { mutableStateOf("") }
    val rtlLabel = stringResource(R.string.rtl_layout)
    val tabs = listOf(R.string.chat, R.string.status, R.string.memory, R.string.journal, R.string.agents, R.string.connection)
    val requestPending = snapshot.objectOrNull("pending") != null
    val busy = requestPending || !localStateAvailable
    val connected = snapshot.text("hub_id").isNotEmpty()
    val uncertain = snapshot.rows("uncertain_operations").filter { it.text("phase") == "unknown" }
    val stateLabel = when (snapshot.text("connection")) {
        "connected" -> R.string.connected; "connecting" -> R.string.connecting; "unknown" -> R.string.unknown; else -> R.string.disconnected
    }
    Scaffold(topBar = { TopAppBar(title = { Text("Hakille · ${stringResource(stateLabel)}") }) }) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            ScrollableTabRow(selectedTabIndex = tab, edgePadding = 8.dp) {
                tabs.forEachIndexed { index, label -> Tab(selected = index == tab, onClick = { tab = index }, text = { Text(stringResource(label)) }) }
            }
            LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                if (restoring) item { Text(stringResource(R.string.draft_restoring), modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
                if (persistenceError.isNotEmpty()) item { Text(persistenceError, color = MaterialTheme.colorScheme.error, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
                if (uncertain.isNotEmpty()) item {
                    Text(stringResource(R.string.recovery_explanation), modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite })
                }
                items(uncertain, key = { "uncertain-${it.text("request_id")}" }) { operation ->
                    Column {
                        Text("${operation.text("kind")} · #${operation.text("request_id")}")
                        TextButton(onClick = { model.dispatch("acknowledge_unknown") { put("request_id", operation.getValue("request_id")) } }, enabled = !busy) { Text(stringResource(R.string.recovery_acknowledge)) }
                    }
                }
                if (error.isNotEmpty()) item { Text(error, color = MaterialTheme.colorScheme.error, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
                snapshot.rows("outcomes").lastOrNull()?.let { outcome -> item { Text("${outcome.text("state")} · ${outcome.text("message")}", modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) } }
                if (requestPending) item {
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                    Text(stringResource(R.string.waiting))
                    TextButton(onClick = model::cancelRequest) { Text(stringResource(R.string.cancel_request)) }
                }
                if (snapshot["history_trimmed"]?.jsonPrimitive?.booleanOrNull == true) item { Text(stringResource(R.string.history_trimmed), style = MaterialTheme.typography.bodySmall) }
                items((snapshot["notifications"] as? JsonArray)?.map { it.jsonPrimitive.content }.orEmpty()) { text -> Text(text, modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }) }
                items(snapshot.rows("cards"), key = { "card-${it.text("id")}" }) { card -> DecisionCard(card, snapshot.objectOrNull("approval"), busy, model) }
                when (tab) {
                    0 -> {
                        items(snapshot.rows("chat")) { row ->
                            Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp)) {
                                Text(stringResource(if (row.text("role") == "user") R.string.you else R.string.assistant), style = MaterialTheme.typography.labelLarge)
                                SelectionContainer { Text(row.text("text")) }
                            } }
                        }
                        item {
                            OutlinedTextField(value = snapshot.text("draft"), onValueChange = { text -> model.dispatch("set_draft") { put("text", text) } }, label = { Text(stringResource(R.string.message)) }, modifier = Modifier.fillMaxWidth(), enabled = !busy)
                            Button(onClick = { model.dispatch("submit") }, enabled = connected && !busy && snapshot.text("draft").isNotBlank()) { Text(stringResource(R.string.send)) }
                            DictationControls(model, enabled = !busy && snapshot.rows("cards").isEmpty())
                        }
                    }
                    1 -> {
                        item { Button(onClick = { model.dispatch("refresh_status") }, enabled = connected && !busy) { Text(stringResource(R.string.refresh)) } }
                        item { JsonDetails(snapshot.objectOrNull("status")) }
                        if (snapshot.rows("outcomes").isNotEmpty()) item { Text(stringResource(R.string.outcomes), style = MaterialTheme.typography.titleMedium) }
                        items(snapshot.rows("outcomes")) { outcome -> SelectionContainer { Text("${outcome.text("state")} · ${outcome.text("message")}\n${outcome["result"] ?: ""}") } }
                    }
                    2 -> {
                        item {
                            OutlinedTextField(search, { search = it }, label = { Text(stringResource(R.string.search)) }, modifier = Modifier.fillMaxWidth())
                            Button(onClick = { model.dispatch("load_memory") { put("query", search.ifBlank { null }); put("limit", 50) } }, enabled = connected && !busy) { Text(stringResource(R.string.search)) }
                        }
                        items(snapshot.rows("memory")) { row -> Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(12.dp)) { SelectionContainer { Text(row.text("text")) }; Text("${row.text("source")} · ${row["tags"] ?: ""}", style = MaterialTheme.typography.bodySmall) } } }
                        item {
                            OutlinedTextField(memoryText, { memoryText = it }, label = { Text(stringResource(R.string.memory_text)) }, modifier = Modifier.fillMaxWidth())
                            Button(onClick = { model.dispatch("remember") { put("text", memoryText) } }, enabled = connected && !busy && memoryText.isNotBlank()) { Text(stringResource(R.string.remember)) }
                        }
                    }
                    3 -> {
                        item { Button(onClick = { model.dispatch("load_journal") { put("limit", 50) } }, enabled = connected && !busy) { Text(stringResource(R.string.refresh)) } }
                        items(snapshot.rows("journal")) { row -> SelectionContainer { Text("${row.text("timestamp")} · ${row.text("action")}\n${row.text("summary")}") } }
                    }
                    4 -> {
                        item { Button(onClick = { model.dispatch("load_agents") }, enabled = connected && !busy) { Text(stringResource(R.string.refresh)) } }
                        items(snapshot.rows("agents"), key = { "agent-${it.text("name")}" }) { agent -> AgentCard(agent, !busy && connected, model) }
                    }
                    5 -> item {
                        Text(stringResource(R.string.connection_help))
                        Row(verticalAlignment = Alignment.CenterVertically) { Text(stringResource(R.string.rtl_layout), Modifier.weight(1f)); Switch(checked = forceRtl, onCheckedChange = changeRtl, modifier = Modifier.semantics { contentDescription = rtlLabel }) }
                        OutlinedTextField(endpoint, { endpoint = it; token = "" }, label = { Text(stringResource(R.string.endpoint)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                        OutlinedTextField(token, { token = it }, label = { Text(stringResource(R.string.token)) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, autoCorrectEnabled = false), modifier = Modifier.fillMaxWidth())
                        Button(onClick = { model.connect(endpoint, token); token = "" }, enabled = endpoint.isNotBlank() && token.isNotBlank() && !busy) { Text(stringResource(R.string.connect)) }
                        if (savedEndpoint.isNotEmpty()) {
                            Text(savedEndpoint)
                            OutlinedButton(onClick = { model.connect("", "", useSaved = true) }, enabled = !busy) { Text(stringResource(R.string.saved_hub)) }
                        }
                        TextButton(onClick = { model.disconnect(); token = "" }, enabled = !restoring) { Text(stringResource(R.string.forget)) }
                    }
                }
                if (!connected && tab != 5) item { Button(onClick = { tab = 5 }) { Text(stringResource(R.string.connection)) } }
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
    val accent = tokenColor(if (isSystemInDarkTheme()) HakilleTokens.colorBraiseDark else HakilleTokens.colorBraiseLight)
    val onAccent = tokenColor(if (isSystemInDarkTheme()) HakilleTokens.colorSurBraiseDark else HakilleTokens.colorSurBraiseLight)
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
