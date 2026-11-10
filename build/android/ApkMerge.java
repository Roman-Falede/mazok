import java.io.FileOutputStream;
import java.nio.file.Files;
import java.nio.file.Paths;
import java.util.Enumeration;
import java.util.zip.CRC32;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import java.util.zip.ZipOutputStream;

/** Добавляет classes.dex в APK от aapt2, сохраняя несжатые записи (resources.arsc и т. п.) несжатыми. */
public class ApkMerge {
    public static void main(String[] a) throws Exception {
        try (ZipFile in = new ZipFile(a[0]); ZipOutputStream out = new ZipOutputStream(new FileOutputStream(a[2]))) {
            Enumeration<? extends ZipEntry> en = in.entries();
            while (en.hasMoreElements()) {
                ZipEntry e = en.nextElement();
                byte[] data = in.getInputStream(e).readAllBytes();
                // aapt2 на Windows пишет пути ассетов с обратной косой чертой — в APK нужны прямые
                out.putNextEntry(entry(e.getName().replace('\\', '/'), data, e.getMethod() == ZipEntry.STORED));
                out.write(data);
                out.closeEntry();
            }
            byte[] dex = Files.readAllBytes(Paths.get(a[1]));
            out.putNextEntry(entry("classes.dex", dex, false));
            out.write(dex);
            out.closeEntry();
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
