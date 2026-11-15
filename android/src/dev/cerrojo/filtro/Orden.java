package dev.cerrojo.filtro;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;

/**
 * Órdenes del PC (adb shell am start -n dev.cerrojo.filtro/.Orden ...):
 *   --es dominios "a.com b.com"   guarda la lista y enciende el filtro
 *   --ez apagar true              lo apaga
 * Solo el shell puede abrirla (pide el permiso DUMP).
 */
public class Orden extends Activity {
    @Override
    protected void onCreate(Bundle b) {
        super.onCreate(b);
        Intent in = getIntent();
        if (in.getBooleanExtra("apagar", false)) {
            Filtro.guardar(this, "", false);
            startService(new Intent(this, Filtro.class).setAction(Filtro.APAGAR));
        } else {
            String d = in.getStringExtra("dominios");
            if (d != null) Filtro.guardar(this, d, true);
            Filtro.encender(this);
        }
        finish();
    }
}
