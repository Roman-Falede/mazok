package app.mazok.rs;

import android.app.NativeActivity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.database.Cursor;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.ImageDecoder;
import android.graphics.Matrix;
import android.media.ExifInterface;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.provider.DocumentsContract;
import android.provider.MediaStore;
import android.provider.OpenableColumns;
import android.view.View;
import android.view.WindowInsets;
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

    // Системные отступы экрана (кнопки навигации, вырез камеры) в пикселях: сверху, снизу, слева, справа.
    private static volatile int insTop, insBottom, insLeft, insRight;

    public static int[] insetsNow() {
        return new int[] { insTop, insBottom, insLeft, insRight };
    }

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        try {
            getWindow().getDecorView().setOnApplyWindowInsetsListener(new View.OnApplyWindowInsetsListener() {
                public WindowInsets onApplyWindowInsets(View v, WindowInsets wi) {
                    int t = wi.getSystemWindowInsetTop(), b = wi.getSystemWindowInsetBottom();
                    int l = wi.getSystemWindowInsetLeft(), r = wi.getSystemWindowInsetRight();
                    if (Build.VERSION.SDK_INT >= 28 && wi.getDisplayCutout() != null) {
                        t = Math.max(t, wi.getDisplayCutout().getSafeInsetTop());
                        b = Math.max(b, wi.getDisplayCutout().getSafeInsetBottom());
                        l = Math.max(l, wi.getDisplayCutout().getSafeInsetLeft());
                        r = Math.max(r, wi.getDisplayCutout().getSafeInsetRight());
                    }
                    insTop = t; insBottom = b; insLeft = l; insRight = r;
                    return v.onApplyWindowInsets(wi);
                }
            });
        } catch (Exception e) {
            // без отступов
        }
        hideSystemUi();
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) hideSystemUi();
    }

    /** Полноэкранный режим: системные кнопки скрыты и появляются по свайпу от края. */
    @SuppressWarnings("deprecation")
    private void hideSystemUi() {
        try {
            getWindow().getDecorView().setSystemUiVisibility(View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_LAYOUT_STABLE | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN);
        } catch (Exception e) {
            // обычный режим
        }
    }

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

    /**
     * Выбор фото: на Android 13+ — системный «Выбор фото» (галерея), раньше — выбор изображений
     * через галерею или файлы. Результат приходит так же, как у openDocuments (код 2).
     */
    public void pickImages(final boolean multiple) {
        runOnUiThread(new Runnable() {
            public void run() {
                if (Build.VERSION.SDK_INT >= 33) {
                    try {
                        Intent p = new Intent(MediaStore.ACTION_PICK_IMAGES);
                        p.setType("image/*");
                        if (multiple) {
                            int max = Math.min(20, MediaStore.getPickImagesMaxLimit());
                            if (max > 1) p.putExtra(MediaStore.EXTRA_PICK_IMAGES_MAX, max);
                        }
                        startActivityForResult(p, 2);
                        return;
                    } catch (Exception e) {
                        // ниже — обычный выбор
                    }
                }
                try {
                    Intent g = new Intent(Intent.ACTION_GET_CONTENT);
                    g.addCategory(Intent.CATEGORY_OPENABLE);
                    g.setType("image/*");
                    g.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple);
                    startActivityForResult(g, 2);
                } catch (Exception e) {
                    try {
                        Intent o = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                        o.addCategory(Intent.CATEGORY_OPENABLE);
                        o.setType("image/*");
                        o.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, multiple);
                        startActivityForResult(o, 2);
                    } catch (Exception e2) {
                        putResult("2\n0\n");
                    }
                }
            }
        });
    }

    /**
     * Декодировать картинку системой: HEIC/HEIF, AVIF и всё, что умеет Android; поворот по EXIF;
     * уменьшение до maxSide по большей стороне. Возвращает JPEG (если нет прозрачности) или PNG; null — ошибка.
     */
    public byte[] decodeImage(String uri, final int maxSide) {
        Bitmap bmp = null;
        try {
            Uri u = Uri.parse(uri);
            if (Build.VERSION.SDK_INT >= 28) {
                ImageDecoder.Source src = ImageDecoder.createSource(getContentResolver(), u);
                bmp = ImageDecoder.decodeBitmap(src, new ImageDecoder.OnHeaderDecodedListener() {
                    public void onHeaderDecoded(ImageDecoder d, ImageDecoder.ImageInfo info, ImageDecoder.Source s) {
                        int w = info.getSize().getWidth(), h = info.getSize().getHeight();
                        int m = Math.max(w, h);
                        if (maxSide > 0 && m > maxSide) {
                            float k = (float) maxSide / m;
                            d.setTargetSize(Math.max(1, Math.round(w * k)), Math.max(1, Math.round(h * k)));
                        }
                        d.setAllocator(ImageDecoder.ALLOCATOR_SOFTWARE);
                    }
                });
            } else {
                BitmapFactory.Options o = new BitmapFactory.Options();
                o.inJustDecodeBounds = true;
                InputStream in = getContentResolver().openInputStream(u);
                if (in == null) return null;
                try { BitmapFactory.decodeStream(in, null, o); } finally { in.close(); }
                int m = Math.max(o.outWidth, o.outHeight);
                int sample = 1;
                while (maxSide > 0 && m / (sample * 2) >= maxSide) sample *= 2;
                BitmapFactory.Options o2 = new BitmapFactory.Options();
                o2.inSampleSize = sample;
                in = getContentResolver().openInputStream(u);
                if (in == null) return null;
                try { bmp = BitmapFactory.decodeStream(in, null, o2); } finally { in.close(); }
                if (bmp == null) return null;
                int rot = 0;
                boolean flip = false;
                in = getContentResolver().openInputStream(u);
                if (in != null) {
                    try {
                        int ori = new ExifInterface(in).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL);
                        switch (ori) {
                            case ExifInterface.ORIENTATION_ROTATE_90: rot = 90; break;
                            case ExifInterface.ORIENTATION_ROTATE_180: rot = 180; break;
                            case ExifInterface.ORIENTATION_ROTATE_270: rot = 270; break;
                            case ExifInterface.ORIENTATION_FLIP_HORIZONTAL: flip = true; break;
                            case ExifInterface.ORIENTATION_FLIP_VERTICAL: flip = true; rot = 180; break;
                            case ExifInterface.ORIENTATION_TRANSPOSE: flip = true; rot = 270; break;
                            case ExifInterface.ORIENTATION_TRANSVERSE: flip = true; rot = 90; break;
                            default: break;
                        }
                    } catch (Exception e) {
                        // без поворота
                    } finally {
                        in.close();
                    }
                }
                int bm = Math.max(bmp.getWidth(), bmp.getHeight());
                float k = maxSide > 0 && bm > maxSide ? (float) maxSide / bm : 1f;
                if (rot != 0 || flip || k < 1f) {
                    Matrix mx = new Matrix();
                    if (k < 1f) mx.postScale(k, k);
                    if (flip) mx.postScale(-1f, 1f);
                    if (rot != 0) mx.postRotate(rot);
                    Bitmap t = Bitmap.createBitmap(bmp, 0, 0, bmp.getWidth(), bmp.getHeight(), mx, true);
                    if (t != bmp) { bmp.recycle(); bmp = t; }
                }
            }
            if (bmp == null) return null;
            ByteArrayOutputStream out = new ByteArrayOutputStream();
            boolean alpha = bmp.hasAlpha();
            bmp.compress(alpha ? Bitmap.CompressFormat.PNG : Bitmap.CompressFormat.JPEG, 95, out);
            return out.toByteArray();
        } catch (Throwable e) {
            return null;
        } finally {
            if (bmp != null) bmp.recycle();
        }
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
