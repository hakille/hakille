package dev.hakille.mobile

import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.TimeUnit

class GatewayTest {
    @Test fun endpointRejectsCredentialLeaksAndRemoteCleartext() {
        for (url in listOf("http://example.com", "http://10.0.2.2:17840", "http://localhost:17840", "https://user:secret@example.com", "https://example.com?token=x", "https://example.com#fragment", "https://example.com/v1/request", "file:///tmp/secret", "https://example.com:0")) {
            assertTrue(url, runCatching { EndpointPolicy.canonical(url) }.isFailure)
        }
        assertEquals("https://example.com", EndpointPolicy.canonical("https://EXAMPLE.com/"))
        assertEquals("http://127.0.0.1:17840", EndpointPolicy.canonical("http://127.0.0.1:17840"))
        assertTrue(runCatching { EndpointPolicy.validateToken("secret\r\nHeader: injected") }.isFailure)
        assertFalse(HubCredentials("https://example.com", "secret").toString().contains("secret"))
    }

    @Test fun protocolIDsRetainAllUnsigned64Bits() {
        val id = "18446744073709551614"
        val frame = Json.parseToJsonElement("{\"id\":$id}").jsonObject
        assertEquals(ULong.MAX_VALUE - 1u, frame.getValue("id").jsonPrimitive.content.toULong())
        assertEquals("{\"id\":$id}", frame.toString())
    }

    @Test fun redirectDoesNotSendTokenToSecondServer() = runBlocking {
        val target = MockWebServer(); val source = MockWebServer()
        target.start(); source.start()
        try {
            source.enqueue(MockResponse().setResponseCode(307).setHeader("Location", target.url("/leak")))
            val result = runCatching { Gateway().send(HubCredentials("http://127.0.0.1:${source.port}", "secret-token"), "{}", null) }
            assertTrue(result.isFailure)
            assertEquals("Bearer secret-token", source.takeRequest(1, TimeUnit.SECONDS)?.getHeader("Authorization"))
            assertEquals(0, target.requestCount)
            assertEquals(1, source.requestCount)
        } finally { source.shutdown(); target.shutdown() }
    }

    @Test fun responseBoundAndErrorEnvelopeArePreserved() = runBlocking {
        val server = MockWebServer(); server.start()
        try {
            val credentials = HubCredentials("http://127.0.0.1:${server.port}", "test-token")
            val error = "{\"session_id\":null,\"frames\":[],\"error\":{\"code\":\"timeout\",\"message\":\"unknown\",\"issue_unknown\":true}}"
            server.enqueue(MockResponse().setResponseCode(504).setBody(error))
            assertEquals(error, Gateway().send(credentials, "{}", "session-123"))
            assertEquals("session-123", server.takeRequest().getHeader("X-Hakille-Session"))
            server.enqueue(MockResponse().setChunkedBody("x".repeat(Gateway.MAX_BYTES + 1), 4096))
            assertTrue(runCatching { Gateway().send(credentials, "{}", null) }.isFailure)
            assertEquals(2, server.requestCount)
        } finally { server.shutdown() }
    }

    @Test fun sendStreamConsumesDeltasAndHandlesTermination() = runBlocking {
        val server = MockWebServer(); server.start()
        try {
            val credentials = HubCredentials("http://127.0.0.1:${server.port}", "test-token")
            val sseBody = "data: {\"frame\":1}\n\ndata: {\"frame\":2}\n\nevent: done\ndata: {}\n\n"
            server.enqueue(MockResponse().setResponseCode(200).setHeader("Content-Type", "text/event-stream").setBody(sseBody))
            val received = mutableListOf<String>()
            Gateway().sendStream(credentials, "{}", "session-456") { chunk ->
                received.add(chunk)
            }
            assertEquals(listOf("{\"frame\":1}", "{\"frame\":2}"), received)
            val request = server.takeRequest()
            assertEquals("text/event-stream", request.getHeader("Accept"))
            assertEquals("session-456", request.getHeader("X-Hakille-Session"))
        } finally { server.shutdown() }
    }
}

