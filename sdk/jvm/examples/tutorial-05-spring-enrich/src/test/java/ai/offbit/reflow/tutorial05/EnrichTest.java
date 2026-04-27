// MockMvc test — boots the full Spring context, posts a request,
// asserts the merged response shape. Each request builds a fresh
// per-request Reflow network, so this exercises the real lifecycle.

package ai.offbit.reflow.tutorial05;

import org.junit.jupiter.api.Test;
import org.springframework.beans.factory.annotation.Autowired;
import org.springframework.boot.test.autoconfigure.web.servlet.AutoConfigureMockMvc;
import org.springframework.boot.test.context.SpringBootTest;
import org.springframework.http.MediaType;
import org.springframework.test.web.servlet.MockMvc;

import static org.springframework.test.web.servlet.request.MockMvcRequestBuilders.post;
import static org.springframework.test.web.servlet.result.MockMvcResultMatchers.*;

@SpringBootTest
@AutoConfigureMockMvc
class EnrichTest {

    @Autowired MockMvc mvc;

    @Test
    void enrichReturnsMergedResponse() throws Exception {
        mvc.perform(post("/enrich")
                .contentType(MediaType.APPLICATION_JSON)
                .content("{\"sku\":\"WIDGET-42\"}"))
            .andExpect(status().isOk())
            .andExpect(jsonPath("$.inventory.sku").value("WIDGET-42"))
            .andExpect(jsonPath("$.inventory.stock").exists())
            .andExpect(jsonPath("$.price.amount").exists())
            .andExpect(jsonPath("$.reviews.count").exists());
    }
}
