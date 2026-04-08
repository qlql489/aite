import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";

const app = createApp(App);
const pinia = createPinia();

// 关闭组件未解析警告
app.config.warnHandler = (msg, _vm, trace) => {
  if (msg.includes('Failed to resolve component')) return;
  console.warn(msg, trace);
};

app.use(pinia);
app.mount("#app");
