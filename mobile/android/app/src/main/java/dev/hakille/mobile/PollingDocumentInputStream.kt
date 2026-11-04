package dev.hakille.mobile

import android.os.CancellationSignal
import android.os.Build
import android.os.ParcelFileDescriptor
import android.os.SystemClock
import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import android.system.StructPollfd
import java.io.IOException
import java.io.InputStream

/** Lecture des pipes SAF sans attente read indéfinie ; le propriétaire ferme aussi le FD à l'annulation.
 * Un fournisseur bloqué pendant son appel Binder/open reste une limite Android :
 * CancellationSignal demande son annulation, sans prétendre arrêter son processus.
 */
class PollingDocumentInputStream(
    private val descriptor: ParcelFileDescriptor,
    private val cancellation: CancellationSignal,
    private val deadlineMillis: Long,
) : InputStream() {
    init {
        val regular = OsConstants.S_ISREG(Os.fstat(descriptor.fileDescriptor).st_mode)
        requireSupportedDocumentDescriptor(Build.VERSION.SDK_INT, regular)
        if (Build.VERSION.SDK_INT >= 30) {
            val flags = Os.fcntlInt(descriptor.fileDescriptor, OsConstants.F_GETFL, 0)
            Os.fcntlInt(descriptor.fileDescriptor, OsConstants.F_SETFL, flags or OsConstants.O_NONBLOCK)
        }
    }
    override fun read(): Int {
        val one = ByteArray(1)
        return if (read(one, 0, 1) < 0) -1 else one[0].toInt() and 255
    }
    override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
        require(offset >= 0 && length >= 0 && offset <= buffer.size - length)
        if (length == 0) return 0
        while (true) {
            cancellation.throwIfCanceled()
            val remaining = deadlineMillis - SystemClock.elapsedRealtime()
            if (remaining <= 0) throw IOException("Délai de lecture local dépassé")
            try {
                val poll = StructPollfd().apply {
                    fd = descriptor.fileDescriptor
                    events = (OsConstants.POLLIN or OsConstants.POLLHUP or OsConstants.POLLERR).toShort()
                }
                if (Os.poll(arrayOf(poll), minOf(remaining, 200).toInt()) == 0) continue
                cancellation.throwIfCanceled()
                if (poll.revents.toInt() and OsConstants.POLLNVAL != 0) throw IOException("Descripteur fermé")
                val count = Os.read(descriptor.fileDescriptor, buffer, offset, length)
                return if (count == 0) -1 else count
            } catch (error: ErrnoException) {
                if (error.errno == OsConstants.EAGAIN || error.errno == OsConstants.EINTR) continue
                throw IOException("Lecture locale interrompue", error)
            }
        }
    }
}
