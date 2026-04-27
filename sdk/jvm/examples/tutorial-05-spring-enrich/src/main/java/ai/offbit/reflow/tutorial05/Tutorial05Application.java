// Spring Boot entry point. Nothing tutorial-specific here — Spring
// auto-discovers the controller via component scan.

package ai.offbit.reflow.tutorial05;

import org.springframework.boot.SpringApplication;
import org.springframework.boot.autoconfigure.SpringBootApplication;

@SpringBootApplication
public class Tutorial05Application {
    public static void main(String[] args) {
        SpringApplication.run(Tutorial05Application.class, args);
    }
}
