package dev.cerrojo.filtro;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.net.ConnectivityManager;
import android.net.LinkProperties;
import android.net.Network;
import android.net.VpnService;
import android.os.Build;
import android.os.ParcelFileDescriptor;

import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Locale;
import java.util.Set;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * VPN local que solo lleva el DNS: el celular pregunta a 10.111.222.1, que
 * existe solo dentro del túnel. Si el nombre (o un dominio padre) está en la
 * lista responde "no existe"; si no, reenvía la pregunta al DNS de la red de
 * siempre. El resto del tráfico no pasa por aquí.
 */
public class Filtro extends VpnService implements Runnable {
    static final String APAGAR = "dev.cerrojo.filtro.APAGAR";
    private static final String PREFS = "filtro";
    private static final String DNS4 = "10.111.222.1", DIR4 = "10.111.222.2";
    private static final String DNS6 = "fd11:c3:7f::1", DIR6 = "fd11:c3:7f::2";
    private static final String[] RESPALDO = {"8.8.8.8", "1.1.1.1"};

    private ParcelFileDescriptor tun;
    private FileOutputStream salida;
    private Thread hilo;
    private final ExecutorService pool = Executors.newFixedThreadPool(8);
    private volatile Set<String> lista = new HashSet<>();
    private volatile Set<String> permitidos = new HashSet<>();
    private volatile List<InetAddress> upstream;
    private volatile long upstreamHasta;

    // ---- Estado guardado ----

    private static SharedPreferences prefs(Context c) {
        return c.getSharedPreferences(PREFS, MODE_PRIVATE);
    }

    static void guardar(Context c, String dominios, boolean activo) {
        prefs(c).edit().putString("dominios", dominios).putBoolean("activo", activo).commit();
    }

    static boolean activo(Context c) {
        return prefs(c).getBoolean("activo", false);
    }

    static void encender(Context c) {
        Intent i = new Intent(c, Filtro.class);
        if (Build.VERSION.SDK_INT >= 26) c.startForegroundService(i);
        else c.startService(i);
    }

    // ---- Servicio ----

    @Override
    public int onStartCommand(Intent in, int flags, int id) {
        primerPlano();
        if ((in != null && APAGAR.equals(in.getAction())) || !activo(this) || prepare(this) != null) {
            // Apagado a propósito, o sin permiso de VPN (lo da el PC con appops).
            parar();
            stopForeground(true);
            stopSelf();
            return START_NOT_STICKY;
        }
        cargarLista();
        if (tun == null) iniciar();
        return START_STICKY;
    }

    private void primerPlano() {
        if (Build.VERSION.SDK_INT < 26) return;
        NotificationManager nm = getSystemService(NotificationManager.class);
        nm.createNotificationChannel(new NotificationChannel("filtro", "Filtro web", NotificationManager.IMPORTANCE_MIN));
        Notification n = new Notification.Builder(this, "filtro")
                .setSmallIcon(android.R.drawable.ic_lock_lock)
                .setContentTitle("Filtro web de Cerrojo")
                .setOngoing(true)
                .build();
        startForeground(1, n);
    }

    private void cargarLista() {
        Set<String> s = new HashSet<>(), ok = new HashSet<>();
        for (String d : prefs(this).getString("dominios", "").split("[\\s,]+")) {
            d = d.trim().toLowerCase(Locale.ROOT);
            boolean excepcion = d.startsWith("!");
            if (excepcion) d = d.substring(1);
            if (d.startsWith("*.")) d = d.substring(2);
            if (!d.isEmpty()) (excepcion ? ok : s).add(d);
        }
        permitidos = ok;
        lista = s;
    }

    private void iniciar() {
        Builder b = new Builder().setSession("Cerrojo").setBlocking(true);
        b.addAddress(DIR4, 32).addDnsServer(DNS4).addRoute(DNS4, 32);
        try {
            b.addAddress(DIR6, 128).addDnsServer(DNS6).addRoute(DNS6, 128);
        } catch (Exception ignored) {
        }
        try {
            // Nuestras preguntas al DNS de verdad no deben volver al túnel.
            b.addDisallowedApplication(getPackageName());
        } catch (Exception ignored) {
        }
        if (Build.VERSION.SDK_INT >= 29) b.setMetered(false);
        tun = b.establish();
        if (tun == null) {
            stopSelf();
            return;
        }
        salida = new FileOutputStream(tun.getFileDescriptor());
        hilo = new Thread(this, "filtro");
        hilo.start();
    }

    private synchronized void parar() {
        if (tun != null) {
            try {
                tun.close();
            } catch (IOException ignored) {
            }
            tun = null;
        }
        if (hilo != null) {
            hilo.interrupt();
            hilo = null;
        }
    }

    @Override
    public void onRevoke() {
        parar();
        super.onRevoke();
    }

    @Override
    public void onDestroy() {
        parar();
        pool.shutdownNow();
        super.onDestroy();
    }

    // ---- Paquetes ----

    @Override
    public void run() {
        ParcelFileDescriptor t = tun;
        if (t == null) return;
        try (FileInputStream in = new FileInputStream(t.getFileDescriptor())) {
            byte[] buf = new byte[32767];
            int n;
            while ((n = in.read(buf)) > 0) {
                try {
                    paquete(Arrays.copyOf(buf, n));
                } catch (RuntimeException ignored) {
                    // Un paquete raro no debe tumbar el filtro.
                }
            }
        } catch (IOException ignored) {
            // Túnel cerrado.
        }
    }

    private void paquete(byte[] p) {
        int ver = (p[0] >> 4) & 0xf;
        int ip, udp;
        byte[] src, dst;
        if (ver == 4) {
            ip = (p[0] & 0xf) * 4;
            if (p[9] != 17) return;
            src = Arrays.copyOfRange(p, 12, 16);
            dst = Arrays.copyOfRange(p, 16, 20);
        } else if (ver == 6) {
            ip = 40;
            if (p[6] != 17) return;
            src = Arrays.copyOfRange(p, 8, 24);
            dst = Arrays.copyOfRange(p, 24, 40);
        } else {
            return;
        }
        udp = ip;
        if (p.length < udp + 8 + 12) return;
        int sport = u16(p, udp), dport = u16(p, udp + 2), len = u16(p, udp + 4);
        if (dport != 53 || len < 8 + 12 || udp + len > p.length) return;
        final byte[] q = Arrays.copyOfRange(p, udp + 8, udp + len);
        String nombre = nombre(q);
        if (nombre == null) return;
        if (bloqueado(nombre)) {
            responder(ver, dst, src, sport, noExiste(q));
            return;
        }
        pool.execute(() -> {
            byte[] r = preguntar(q);
            if (r != null) responder(ver, dst, src, sport, r);
        });
    }

    private boolean bloqueado(String n) {
        return hay(lista, n) && !hay(permitidos, n);
    }

    /** ¿Está n, o un dominio padre de n, en l? */
    private static boolean hay(Set<String> l, String n) {
        while (true) {
            if (l.contains(n)) return true;
            int i = n.indexOf('.');
            if (i < 0) return false;
            n = n.substring(i + 1);
        }
    }

    /** El nombre de la primera pregunta, o null. */
    private static String nombre(byte[] q) {
        if (u16(q, 4) < 1) return null;
        StringBuilder sb = new StringBuilder();
        int i = 12;
        while (i < q.length) {
            int l = q[i] & 0xff;
            if (l == 0) return sb.toString().toLowerCase(Locale.ROOT);
            if (l > 63 || i + 1 + l > q.length) return null;
            if (sb.length() > 0) sb.append('.');
            sb.append(new String(q, i + 1, l, java.nio.charset.StandardCharsets.ISO_8859_1));
            i += 1 + l;
        }
        return null;
    }

    /** Respuesta NXDOMAIN: la cabecera y la pregunta, sin nada más. */
    private static byte[] noExiste(byte[] q) {
        int i = 12;
        while (i < q.length && q[i] != 0) i += 1 + (q[i] & 0xff);
        int fin = Math.min(q.length, i + 1 + 4);
        byte[] r = Arrays.copyOf(q, fin);
        r[2] = (byte) (0x80 | (q[2] & 0x79)); // QR, opcode y RD
        r[3] = (byte) (0x80 | 3); // RA, NXDOMAIN
        r[4] = 0;
        r[5] = 1;
        for (int k = 6; k < 12; k++) r[k] = 0;
        return r;
    }

    private byte[] preguntar(byte[] q) {
        for (InetAddress s : servidores()) {
            try (DatagramSocket sock = new DatagramSocket()) {
                protect(sock);
                sock.setSoTimeout(4000);
                sock.send(new DatagramPacket(q, q.length, s, 53));
                byte[] buf = new byte[4096];
                DatagramPacket r = new DatagramPacket(buf, buf.length);
                sock.receive(r);
                return Arrays.copyOf(buf, r.getLength());
            } catch (IOException ignored) {
                // Probar el siguiente.
            }
        }
        return null;
    }

    /** El DNS de la red real (wifi o datos); si no se sabe, uno público. */
    private List<InetAddress> servidores() {
        long ahora = System.currentTimeMillis();
        List<InetAddress> l = upstream;
        if (l != null && ahora < upstreamHasta) return l;
        l = new ArrayList<>();
        try {
            ConnectivityManager cm = getSystemService(ConnectivityManager.class);
            Network n = cm.getActiveNetwork();
            LinkProperties lp = n == null ? null : cm.getLinkProperties(n);
            if (lp != null) {
                for (InetAddress a : lp.getDnsServers()) {
                    String h = a.getHostAddress();
                    if (!DNS4.equals(h) && !h.startsWith("fd11:c3:7f")) l.add(a);
                }
            }
        } catch (Exception ignored) {
        }
        for (String r : RESPALDO) {
            try {
                l.add(InetAddress.getByName(r));
            } catch (Exception ignored) {
            }
        }
        upstream = l;
        upstreamHasta = ahora + 15000;
        return l;
    }

    private void responder(int ver, byte[] src, byte[] dst, int dport, byte[] dns) {
        int ulen = 8 + dns.length;
        byte[] p;
        int u;
        if (ver == 4) {
            p = new byte[20 + ulen];
            p[0] = 0x45;
            put16(p, 2, p.length);
            p[6] = 0x40;
            p[8] = 64;
            p[9] = 17;
            System.arraycopy(src, 0, p, 12, 4);
            System.arraycopy(dst, 0, p, 16, 4);
            put16(p, 10, suma(p, 0, 20, 0));
            u = 20;
        } else {
            p = new byte[40 + ulen];
            p[0] = 0x60;
            put16(p, 4, ulen);
            p[6] = 17;
            p[7] = 64;
            System.arraycopy(src, 0, p, 8, 16);
            System.arraycopy(dst, 0, p, 24, 16);
            u = 40;
        }
        put16(p, u, 53);
        put16(p, u + 2, dport);
        put16(p, u + 4, ulen);
        System.arraycopy(dns, 0, p, u + 8, dns.length);
        if (ver == 6) {
            // En IPv6 la suma de UDP es obligatoria: pseudo-cabecera + UDP.
            long acc = 0;
            for (int i = 8; i < 40; i += 2) acc += u16(p, i);
            acc += ulen + 17;
            int c = suma(p, u, ulen, acc);
            put16(p, u + 6, c == 0 ? 0xffff : c);
        }
        synchronized (this) {
            if (salida == null || tun == null) return;
            try {
                salida.write(p);
            } catch (IOException ignored) {
            }
        }
    }

    private static int suma(byte[] b, int off, int len, long acc) {
        for (int i = 0; i + 1 < len; i += 2) acc += u16(b, off + i);
        if ((len & 1) == 1) acc += (b[off + len - 1] & 0xff) << 8;
        while ((acc >> 16) != 0) acc = (acc & 0xffff) + (acc >> 16);
        return (int) (~acc & 0xffff);
    }

    private static int u16(byte[] b, int i) {
        return ((b[i] & 0xff) << 8) | (b[i + 1] & 0xff);
    }

    private static void put16(byte[] b, int i, int v) {
        b[i] = (byte) (v >> 8);
        b[i + 1] = (byte) v;
    }
}
