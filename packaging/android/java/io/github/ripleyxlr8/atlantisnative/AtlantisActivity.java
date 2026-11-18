// Atlantis Native's activity (docs/ANDROID.md): Android's NativeActivity,
// which runs the Rust code (libatlantis_app.so), with what NativeActivity
// cannot do from native code alone:
// - the system's folder picker (ACTION_OPEN_DOCUMENT_TREE), its answer
//   passed to the Rust code (folderPicked, registered from Rust), and the
//   folder's documents listed and opened (the Storage Access Framework);
// - the full screen without the status and navigation bars (immersive).
// The Rust code calls these methods through JNI (crates/atlantis-app/src/
// android_saf.rs).
package io.github.ripleyxlr8.atlantisnative;

import android.app.NativeActivity;
import android.content.ContentResolver;
import android.content.Intent;
import android.content.UriPermission;
import android.database.Cursor;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.ParcelFileDescriptor;
import android.provider.DocumentsContract;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import java.util.ArrayList;

public class AtlantisActivity extends NativeActivity {
    private static final int PICK_FOLDER = 1;

    /** The folder chosen in the picker (its tree URI), "" when none was. */
    private static native void folderPicked(String treeUri);

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        immersive();
    }

    @Override
    protected void onResume() {
        super.onResume();
        immersive();
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            immersive();
        }
    }

    /** The game over the whole screen: no status bar, no navigation bar
     *  (a swipe from the edge shows them for a moment), the display cutout
     *  used. */
    @SuppressWarnings("deprecation")
    private void immersive() {
        Window w = getWindow();
        if (Build.VERSION.SDK_INT >= 28) {
            WindowManager.LayoutParams lp = w.getAttributes();
            lp.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            w.setAttributes(lp);
        }
        if (Build.VERSION.SDK_INT >= 30) {
            w.setDecorFitsSystemWindows(false);
            WindowInsetsController c = w.getInsetsController();
            if (c != null) {
                c.hide(WindowInsets.Type.systemBars());
                c.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            w.getDecorView().setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                            | View.SYSTEM_UI_FLAG_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        }
    }

    /** Opens the system's folder picker (from any thread); the answer
     *  comes to folderPicked. */
    public void pickFolder() {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
                i.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION
                        | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
                try {
                    startActivityForResult(i, PICK_FOLDER);
                } catch (Exception e) {
                    folderPicked("");
                }
            }
        });
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != PICK_FOLDER) {
            return;
        }
        String uri = "";
        if (resultCode == RESULT_OK && data != null && data.getData() != null) {
            Uri tree = data.getData();
            try {
                // Kept across restarts of the app.
                getContentResolver().takePersistableUriPermission(
                        tree, Intent.FLAG_GRANT_READ_URI_PERMISSION);
            } catch (SecurityException e) {
                // (read for this run only)
            }
            uri = tree.toString();
        }
        folderPicked(uri);
    }

    /** Whether the app may still read the folder (its permission kept). */
    public boolean canRead(String treeUri) {
        for (UriPermission p : getContentResolver().getPersistedUriPermissions()) {
            if (p.isReadPermission() && p.getUri().toString().equals(treeUri)) {
                return true;
            }
        }
        return false;
    }

    /** The document id of the folder itself. */
    public String rootDocument(String treeUri) {
        try {
            return DocumentsContract.getTreeDocumentId(Uri.parse(treeUri));
        } catch (Exception e) {
            return null;
        }
    }

    /** The documents in folder `documentId`: "name\tid\tfolder(1/0)\tsize"
     *  each; null when the folder cannot be read. */
    public String[] listChildren(String treeUri, String documentId) {
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(
                Uri.parse(treeUri), documentId);
        String[] columns = {
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE,
        };
        ArrayList<String> out = new ArrayList<>();
        ContentResolver r = getContentResolver();
        try (Cursor c = r.query(children, columns, null, null, null)) {
            if (c == null) {
                return null;
            }
            while (c.moveToNext()) {
                boolean dir = DocumentsContract.Document.MIME_TYPE_DIR.equals(c.getString(2));
                long size = c.isNull(3) ? 0 : c.getLong(3);
                out.add(c.getString(0) + "\t" + c.getString(1) + "\t" + (dir ? "1" : "0")
                        + "\t" + size);
            }
        } catch (Exception e) {
            return null;
        }
        return out.toArray(new String[0]);
    }

    /** A file descriptor of document `documentId`, read only, now owned by
     *  the caller; -1 when it cannot be opened. */
    public int openDocument(String treeUri, String documentId) {
        try {
            Uri doc = DocumentsContract.buildDocumentUriUsingTree(Uri.parse(treeUri), documentId);
            ParcelFileDescriptor p = getContentResolver().openFileDescriptor(doc, "r");
            return p == null ? -1 : p.detachFd();
        } catch (Exception e) {
            return -1;
        }
    }
}
