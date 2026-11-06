package dev.hakille.mobile

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import kotlinx.serialization.json.*

/** Endpoint and token are encrypted together, so a saved token cannot drift to another hub. */
class CredentialStore(context: Context) {
    private val prefs = context.getSharedPreferences("hub_credentials", Context.MODE_PRIVATE)
    private val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
    private fun key(): SecretKey {
        (keyStore.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256).build())
        }.generateKey()
    }
    fun save(credentials: HubCredentials) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val payload = buildJsonObject { put("endpoint", credentials.endpoint); put("token", credentials.token) }.toString()
        val encrypted = cipher.doFinal(payload.toByteArray(Charsets.UTF_8))
        check(prefs.edit().putString("ciphertext", Base64.encodeToString(encrypted, Base64.NO_WRAP))
            .putString("iv", Base64.encodeToString(cipher.iv, Base64.NO_WRAP)).commit()) { "Credential storage failed / Échec du stockage sécurisé" }
    }
    fun load(): HubCredentials? {
        val bytes = prefs.getString("ciphertext", null) ?: return null
        val iv = prefs.getString("iv", null) ?: error("Encrypted credentials incomplete / Identifiants chiffrés incomplets")
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply {
            init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, Base64.decode(iv, Base64.NO_WRAP)))
        }
        val objectValue = Json.parseToJsonElement(cipher.doFinal(Base64.decode(bytes, Base64.NO_WRAP)).toString(Charsets.UTF_8)).jsonObject
        return HubCredentials(objectValue.getValue("endpoint").jsonPrimitive.content, objectValue.getValue("token").jsonPrimitive.content)
    }
    fun clear() { check(prefs.edit().clear().commit()); if (keyStore.containsAlias(ALIAS)) keyStore.deleteEntry(ALIAS) }
    private companion object { const val ALIAS = "hakille.gateway.v1" }
}
