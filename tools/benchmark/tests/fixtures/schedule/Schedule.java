import java.util.ArrayList;
import java.util.Collections;
import java.util.Random;
// Pure scheduling probe: no validator, inputs, timers, filesystem or downloads.
public final class Schedule {
    public static void main(String[] args) {
        for (int run = 0; run < 5; run++) {
            var order = new ArrayList<Integer>();
            for (int slot = 0; slot < 41; slot++) order.add(slot);
            Collections.shuffle(order, new Random(271828L + run));
            System.out.println("{\"run\":" + run + ",\"seed\":" + (271828L + run)
                + ",\"manifest_slots\":" + order + "}");
        }
    }
}
