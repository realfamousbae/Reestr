// @ts-check
import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";

// The user manual of Reestr. Russian is the root locale: pages live at "/start/…", not "/ru/…".
// No analytics, no external fonts or scripts: the site is as quiet as the app.
export default defineConfig({
  site: "https://reestr-docs.up.railway.app",
  integrations: [
    starlight({
      title: "Реестр — руководство",
      description: "Как пользоваться Реестром: семейный журнал трат, карт и кэшбэка.",
      defaultLocale: "root",
      locales: {
        root: { label: "Русский", lang: "ru" },
      },
      customCss: [
        "@fontsource/pt-sans/400.css",
        "@fontsource/pt-sans/700.css",
        "./src/styles/theme.css",
      ],
      sidebar: [
        { label: "Начало", items: [{ autogenerate: { directory: "start" } }] },
        { label: "Работа каждый день", items: [{ autogenerate: { directory: "guide" } }] },
        { label: "Справочник", items: [{ autogenerate: { directory: "reference" } }] },
        { label: "Помощь", items: [{ autogenerate: { directory: "help" } }] },
      ],
    }),
  ],
});
