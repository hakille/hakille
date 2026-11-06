package dev.hakille.mobile

import java.math.BigDecimal
import java.text.NumberFormat
import java.util.Locale
import kotlinx.serialization.json.*

object CardPresentation {
    fun money(value: JsonElement?, locale: Locale = Locale.getDefault()): String {
        val amount = (value as? JsonObject)?.get("micros")?.jsonPrimitive?.content?.toBigDecimalOrNull()
            ?: return "?"
        val currency = (value as? JsonObject)?.text("currency").orEmpty()
        val format = NumberFormat.getNumberInstance(locale).apply { minimumFractionDigits = 2; maximumFractionDigits = 6 }
        return "${format.format(amount.divide(BigDecimal(1_000_000)))} $currency"
    }
    fun destination(value: JsonElement?, local: String, cloud: String, unknown: String): String {
        if ((value as? JsonPrimitive)?.contentOrNull == "Local") return local
        val provider = (value as? JsonObject)?.objectOrNull("Cloud")?.text("provider")
        return if (!provider.isNullOrEmpty()) "$cloud : $provider" else unknown
    }
    fun provenance(value: JsonElement?, external: String, verified: String, unknown: String): String {
        return when ((value as? JsonPrimitive)?.contentOrNull) {
            "Hakille" -> "Hakille"
            "External" -> external
            else -> {
                val skill = (value as? JsonObject)?.objectOrNull("VerifiedSkill")
                if (skill != null) "$verified : ${skill.text("publisher")} / ${skill.text("name")}" else unknown
            }
        }
    }
}
