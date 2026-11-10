package app.mazok.rs;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.provider.DocumentsContract;
import android.provider.OpenableColumns;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * Активность «Мазка»: всё рисование — в нативной библиотеке (Rust),
 * здесь только системный выбор файлов и папок (Storage Access Framework) и буфер обмена.
 * Результат выбора забирается из Rust вызовом takeResult().
 */
public class MazokActivity extends NativeActivity {
    private static final Object LOCK = new Object();
    private static String result = null;

    public static String takeResult() {
        synchronized (LOCK) {
            String r = result;
            result = null;
            return r;
        }
    }

    private static void putResult(String r) {
        synchronized (LOCK) {
            result = r;
        }
    }

    private void start(final Intent intent, final int req) {
        runOnUiThread(new Runnable() {
            public void run() {
                try {
                    startActivityForResult(intent, req);
                } catch (Exception e) {
                    putResult(req + "\n0\n");
                }
            }
        });
    }

    /** Диалог «Сохранить как»: пользователь выбирает папку и имя. */
    public void createDocument(String name, String mime) {
        Intent i = new Intent(Intent.ACTION_CREATE_DOCUMENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);
        i.setType(mime == null || mime.isEmpty() ? "application/octet-stream" : mime);
        i.putExtra(Intent.EXTRA_TITLE, name);
        start(i, 1);
    }

    /** Выбор файлов; mimes — через запятую. */
    public void openDocuments(String mimes, boolean multiple) {
        Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
        i.addCategory(Intent.CATEGORY_OPENABLE);
        if (mimes != null && mimes.length() > 0 && !mimes.equals("*/*")) {
            String[] list = mimes.split(",");
            i.setType(list.length == 1 ? list[0] : "*/*");
            if (list.length > 1) i.putExtra(Intent.EXTRA_MIME_TYPES, list);
        } else {
            i.setType("*/*");
        }
        i.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple);
        start(i, 2);
    }

    /** Выбор папки для сохранения (доступ запоминается). */
    public void openTree() {
        Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        i.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
        start(i, 3);
    }

    @Override
    protected void onActivityResult(int req, int code, Intent data) {
        super.onActivityResult(req, code, data);
        StringBuilder sb = new StringBuilder();
        boolean ok = code == RESULT_OK && data != null;
        if (ok) {
            ClipData clip = data.getClipData();
            if (clip != null && clip.getItemCount() > 0) {
                for (int k = 0; k < clip.getItemCount(); k++) {
                    Uri u = clip.getItemAt(k).getUri();
                    if (u != null) sb.append(u.toString()).append('\n');
                }
            } else if (data.getData() != null) {
                sb.append(data.getData().toString()).append('\n');
            }
            if (req == 3 && data.getData() != null) {
                try {
                    getContentResolver().takePersistableUriPermission(data.getData(), Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION);
                } catch (Exception e) {
                    // папка всё равно доступна до перезапуска
                }
            }
        }
        putResult(req + "\n" + (ok ? 1 : 0) + "\n" + sb.toString());
    }

    public String displayName(String uri) {
        Cursor c = null;
        try {
            c = getContentResolver().query(Uri.parse(uri), new String[] { OpenableColumns.DISPLAY_NAME }, null, null, null);
            if (c != null && c.moveToFirst()) {
                String s = c.getString(0);
                if (s != null) return s;
            }
        } catch (Exception e) {
            // ниже — имя из адреса
        } finally {
            if (c != null) c.close();
        }
        String s = Uri.parse(uri).getLastPathSegment();
        return s == null ? "file" : s;
    }

    public byte[] readUri(String uri) {
        InputStream in = null;
        try {
            in = getContentResolver().openInputStream(Uri.parse(uri));
            if (in == null) return null;
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            byte[] buf = new byte[1 << 16];
            int n;
            while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
            return out.toByteArray();
        } catch (Exception e) {
            return null;
        } finally {
            try { if (in != null) in.close(); } catch (Exception e) { }
        }
    }

    public boolean writeUri(String uri, byte[] data) {
        String[] modes = { "wt", "w" };
        for (String m : modes) {
            OutputStream os = null;
            try {
                os = getContentResolver().openOutputStream(Uri.parse(uri), m);
                if (os == null) continue;
                os.write(data);
                os.flush();
                return true;
            } catch (Exception e) {
                // пробуем следующий режим
            } finally {
                try { if (os != null) os.close(); } catch (Exception e) { }
            }
        }
        return false;
    }

    /** Создать файл в выбранной ранее папке; возвращает его адрес. */
    public String createInTree(String tree, String name, String mime) {
        try {
            Uri t = Uri.parse(tree);
            Uri parent = DocumentsContract.buildDocumentUriUsingTree(t, DocumentsContract.getTreeDocumentId(t));
            Uri u = DocumentsContract.createDocument(getContentResolver(), parent, mime, name);
            return u == null ? null : u.toString();
        } catch (Exception e) {
            return null;
        }
    }

    /** Картинка из буфера обмена (если там адрес изображения). */
    public byte[] clipboardImage() {
        try {
            ClipboardManager cm = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
            if (cm == null || !cm.hasPrimaryClip()) return null;
            ClipData c = cm.getPrimaryClip();
            if (c == null) return null;
            for (int k = 0; k < c.getItemCount(); k++) {
                Uri u = c.getItemAt(k).getUri();
                if (u != null) {
                    byte[] b = readUri(u.toString());
                    if (b != null) return b;
                }
            }
        } catch (Exception e) {
            // буфер недоступен
        }
        return null;
    }
}
