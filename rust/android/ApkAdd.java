import java.io.FileOutputStream;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.Enumeration;
import java.util.zip.CRC32;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import java.util.zip.ZipOutputStream;

/**
 * Копирует APK от aapt2 и добавляет файлы: java ApkAdd.java in.apk out.apk путь/в/apk=файл ...
 * Несжатые записи остаются несжатыми; нативные библиотеки (.so) кладутся без сжатия,
 * чтобы Android загружал их прямо из APK (extractNativeLibs=false).
 */
public class ApkAdd {
    public static void main(String[] a) throws Exception {
        try (ZipFile in = new ZipFile(a[0]); ZipOutputStream out = new ZipOutputStream(new FileOutputStream(a[1]))) {
            Enumeration<? extends ZipEntry> en = in.entries();
            while (en.hasMoreElements()) {
                ZipEntry e = en.nextElement();
                byte[] data = in.getInputStream(e).readAllBytes();
                out.putNextEntry(entry(e.getName().replace('\\', '/'), data, e.getMethod() == ZipEntry.STORED));
                out.write(data);
                out.closeEntry();
            }
            for (int i = 2; i < a.length; i++) {
                int k = a[i].indexOf('=');
                String name = a[i].substring(0, k);
                byte[] data = Files.readAllBytes(Paths.get(a[i].substring(k + 1)));
                out.putNextEntry(entry(name, data, name.endsWith(".so")));
                out.write(data);
                out.closeEntry();
            }
        }
    }

    static ZipEntry entry(String name, byte[] data, boolean stored) {
        ZipEntry n = new ZipEntry(name);
        if (stored) {
            CRC32 c = new CRC32();
            c.update(data);
            n.setMethod(ZipEntry.STORED);
            n.setSize(data.length);
            n.setCompressedSize(data.length);
            n.setCrc(c.getValue());
        } else n.setMethod(ZipEntry.DEFLATED);
        return n;
    }
}
