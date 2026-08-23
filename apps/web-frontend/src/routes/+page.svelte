<script lang="ts">
    import { onMount } from "svelte";
    import { replaceState } from "$app/navigation";
    import BrandMark from "$lib/components/BrandMark.svelte";
    import LocaleSwitch from "$lib/components/LocaleSwitch.svelte";
    import { locale, setLocale, type AppLocale } from "$lib/i18n";

    const webAppUrl =
        (import.meta.env.VITE_KAMORI_WEB_APP_URL as string | undefined)?.trim() ||
        "/app";

    export let data: { requestedLocale: AppLocale | null };

    const repository = "https://github.com/Kamori-app/kamori-app";
    const links = {
        releases: `${repository}/releases`,
        license: `${repository}/blob/main/LICENSE.md`,
        security: `${repository}/blob/main/SECURITY.md`,
        architecture: `${repository}/blob/main/docs/architecture/overview.md`,
        roadmap: `${repository}/blob/main/docs/ROADMAP.md`,
        desktopGuide: `${repository}/blob/main/docs/whitepapers/desktop-dav-bridge.md`,
        desktopGuideRu: `${repository}/blob/main/docs/whitepapers/desktop-dav-bridge.ru.md`,
        mobileGuide: `${repository}/blob/main/docs/whitepapers/mobile-system-integration.md`,
        mobileGuideRu: `${repository}/blob/main/docs/whitepapers/mobile-system-integration.ru.md`,
        privacy: `${repository}/blob/main/docs/whitepapers/security-and-privacy.md`,
        privacyRu: `${repository}/blob/main/docs/whitepapers/security-and-privacy.ru.md`,
        selfHost: `${repository}/blob/main/apps/cloud-server/README.md`,
        buildDesktop: `${repository}/blob/main/apps/dav-bridge-desktop/README.md`,
        buildMobile: `${repository}/blob/main/apps/dav-bridge-mobile/README.md`,
    };

    const copies = {
        en: {
            skip: "Skip to content",
            alpha: "Public alpha",
            nav: [
                ["product", "Product"],
                ["how-it-works", "How it works"],
                ["apps", "Apps"],
                ["security", "Security"],
                ["questions", "Questions"],
            ],
            openApp: "Open web app",
            openMenu: "Open navigation",
            closeMenu: "Close navigation",
            eyebrow: "Private work, without a readable cloud copy",
            hero: "Your calendar, contacts and tasks belong to your devices.",
            intro:
                "Kamori is an offline-first personal organizer. Its first-party apps encrypt every operation before sync; the hosted service only moves ciphertext between your devices.",
            start: "Create an account",
            seeApps: "See the apps",
            available: "Web · Desktop · Android · iOS",
            noteTitle: "One account, four surfaces",
            noteBody:
                "Work directly in Kamori on the web or mobile. On desktop, use the native control center—or expose an optional local CalDAV/CardDAV bridge to an app you already trust.",
            productLabel: "01 / Product",
            productTitle: "An organizer first. A compatibility bridge second.",
            productBody:
                "The encrypted operation log is the source of truth. DAV never defines the cloud model; it is a local desktop projection for interoperability.",
            productItems: [
                ["Native by default", "Calendars, tasks and contacts live in first-party Kamori clients, with offline editing built in."],
                ["Optional system access", "Android and iOS can project selected collections into system calendars or contacts only after explicit consent."],
                ["DAV when useful", "The desktop app can provide a localhost-only CalDAV/CardDAV endpoint for established desktop clients."],
            ],
            approachLabel: "02 / How it works",
            approachTitle: "Plaintext stops at the device boundary.",
            paths: [
                ["First-party apps", "Edit offline on web, desktop or mobile"],
                ["Device crypto", "Sign and encrypt operations locally"],
                ["Kamori Cloud", "Store and relay opaque encrypted records"],
                ["Your other devices", "Verify, decrypt and merge locally"],
            ],
            bridgeAside: "Optional desktop side path",
            bridgePath: "Calendar / contacts app  ↔  local DAV bridge  ↔  device crypto",
            architecture: "Read the architecture overview",
            downloadsLabel: "03 / Apps",
            downloadsTitle: "Choose how you want to work.",
            downloadsBody:
                "Registration starts on the web. Desktop and mobile apps are sign-in only and use the same encrypted account state.",
            webTitle: "Web organizer",
            webBody: "The fastest way to start. First-party calendar, task and contact views, available from any modern browser.",
            webAction: "Use Kamori on the web",
            desktopTitle: "Desktop control center",
            desktopBody: "Native organizer and sync controls for macOS, Windows and Linux, plus the optional local DAV bridge.",
            desktopAction: "Desktop releases",
            mobileTitle: "Mobile organizer",
            mobileBody: "Offline-first Android and iOS apps. Optional system projection is per collection and can be disabled again.",
            mobileAction: "Mobile releases",
            releasePrefix: "Latest published release",
            releaseNotes: "release notes",
            desktopPaper: "Desktop DAV white paper",
            mobilePaper: "Mobile integration white paper",
            buildSource: "Build instructions",
            securityLabel: "04 / Security",
            securityTitle: "Claims should be inspectable.",
            securityBody:
                "Kamori uses OPAQUE password authentication, passkeys on the web, device-held keys and a signed encrypted operation log. We document the limits too: timing, membership and traffic volume can remain visible to the service.",
            securityLinks: ["Security & privacy paper", "Architecture", "Responsible disclosure", "Roadmap"],
            sharingTitle: "Sharing without permanent links",
            sharingBody:
                "Invite codes are single-use. You choose an expiry from 15 minutes to 7 days; collection keys and optional notes remain client-encrypted.",
            questionsLabel: "05 / Questions",
            questionsTitle: "The short version.",
            faq: [
                ["Can Kamori read my organizer data?", "The hosted service stores encrypted operations and blobs. Decryption keys remain on authorized devices."],
                ["Do I need a DAV application?", "No. The web, desktop and mobile apps are full first-party clients. DAV is an optional desktop compatibility feature."],
                ["Does mobile run a localhost server?", "No. Android and iOS use native system calendar/contact APIs for optional projection; Kamori does not keep a mobile localhost DAV server alive."],
                ["Can I self-host?", "The server and web client are AGPL-3.0-only. Self-hosting is planned after the hosted MVP; the current server documentation is available for contributors."],
            ],
            source: "Corresponding source",
            license: "License",
            footer:
                "Kamori is early software. Keep independent exports of important data and review release notes before upgrading.",
        },
        ru: {
            skip: "К содержанию",
            alpha: "Публичная альфа",
            nav: [
                ["product", "Продукт"],
                ["how-it-works", "Как это работает"],
                ["apps", "Приложения"],
                ["security", "Безопасность"],
                ["questions", "Вопросы"],
            ],
            openApp: "Открыть веб-приложение",
            openMenu: "Открыть навигацию",
            closeMenu: "Закрыть навигацию",
            eyebrow: "Личные данные — без читаемой копии в облаке",
            hero: "Календарь, контакты и задачи принадлежат вашим устройствам.",
            intro:
                "Kamori — офлайн-органайзер. Официальные приложения шифруют каждую операцию до синхронизации; hosted-сервис только передаёт шифротекст между вашими устройствами.",
            start: "Создать аккаунт",
            seeApps: "Посмотреть приложения",
            available: "Веб · Desktop · Android · iOS",
            noteTitle: "Один аккаунт, четыре интерфейса",
            noteBody:
                "Работайте прямо в Kamori через веб или мобильное приложение. На компьютере используйте нативный центр управления — либо включите локальный CalDAV/CardDAV-мост для привычного приложения.",
            productLabel: "01 / Продукт",
            productTitle: "Сначала органайзер. Затем — мост совместимости.",
            productBody:
                "Источник истины — зашифрованный журнал операций. DAV не определяет облачную модель: это только локальная desktop-проекция для совместимости.",
            productItems: [
                ["Нативно по умолчанию", "Календари, задачи и контакты доступны в официальных клиентах Kamori с полноценной офлайн-работой."],
                ["Системный доступ по желанию", "Android и iOS проецируют выбранные коллекции в системный календарь или контакты только после явного согласия."],
                ["DAV, когда он полезен", "Desktop-приложение может открыть localhost-only CalDAV/CardDAV endpoint для привычных программ."],
            ],
            approachLabel: "02 / Как это работает",
            approachTitle: "Открытые данные не покидают устройство.",
            paths: [
                ["Официальные приложения", "Редактирование офлайн в вебе, desktop или mobile"],
                ["Криптография на устройстве", "Локальная подпись и шифрование операций"],
                ["Kamori Cloud", "Хранение и передача непрозрачных зашифрованных записей"],
                ["Другие ваши устройства", "Локальная проверка, расшифровка и слияние"],
            ],
            bridgeAside: "Дополнительный desktop-маршрут",
            bridgePath: "Календарь / контакты  ↔  локальный DAV-мост  ↔  криптография устройства",
            architecture: "Открыть обзор архитектуры",
            downloadsLabel: "03 / Приложения",
            downloadsTitle: "Работайте удобным способом.",
            downloadsBody:
                "Регистрация начинается в вебе. Desktop и mobile предназначены только для входа и используют то же зашифрованное состояние аккаунта.",
            webTitle: "Веб-органайзер",
            webBody: "Самый быстрый старт. Официальные календари, задачи и контакты в любом современном браузере.",
            webAction: "Открыть Kamori в вебе",
            desktopTitle: "Desktop-центр управления",
            desktopBody: "Нативный органайзер и управление синхронизацией на macOS, Windows и Linux, плюс необязательный локальный DAV-мост.",
            desktopAction: "Desktop-релизы",
            mobileTitle: "Мобильный органайзер",
            mobileBody: "Офлайн-приложения для Android и iOS. Системная проекция включается отдельно для коллекций и всегда может быть отключена.",
            mobileAction: "Mobile-релизы",
            releasePrefix: "Последний опубликованный релиз",
            releaseNotes: "заметки к релизу",
            desktopPaper: "White paper о desktop DAV-мосте",
            mobilePaper: "White paper о мобильной интеграции",
            buildSource: "Инструкция по сборке",
            securityLabel: "04 / Безопасность",
            securityTitle: "Заявления должны быть проверяемыми.",
            securityBody:
                "Kamori использует OPAQUE-аутентификацию по паролю, passkeys в вебе, ключи на устройствах и подписанный зашифрованный журнал операций. Мы документируем и ограничения: сервису могут быть видны время, состав участников и объём трафика.",
            securityLinks: ["White paper о безопасности", "Архитектура", "Сообщить об уязвимости", "Роадмап"],
            sharingTitle: "Шаринг без вечных ссылок",
            sharingBody:
                "Коды приглашений одноразовые. Вы выбираете срок от 15 минут до 7 дней; ключи коллекции и необязательная заметка остаются зашифрованными на клиенте.",
            questionsLabel: "05 / Вопросы",
            questionsTitle: "Коротко о главном.",
            faq: [
                ["Может ли Kamori читать мои данные?", "Hosted-сервис хранит зашифрованные операции и файлы. Ключи расшифровки остаются на авторизованных устройствах."],
                ["Мне обязательно нужно DAV-приложение?", "Нет. Веб, desktop и mobile — полноценные официальные клиенты. DAV нужен только как дополнительная desktop-совместимость."],
                ["На мобильном работает localhost-сервер?", "Нет. Android и iOS используют системные API календаря и контактов для необязательной проекции; Kamori не держит мобильный localhost DAV-сервер."],
                ["Можно ли развернуть Kamori самостоятельно?", "Сервер и веб-клиент распространяются под AGPL-3.0-only. Self-hosting запланирован после hosted MVP; текущая серверная документация доступна контрибьюторам."],
            ],
            source: "Исходный код",
            license: "Лицензия",
            footer:
                "Kamori пока находится на ранней стадии. Храните независимые экспорты важных данных и читайте заметки перед обновлением.",
        },
    } as const;

    let currentLocale: AppLocale = data.requestedLocale ?? "en";

    $: c = copies[currentLocale];
    $: desktopPaper = currentLocale === "ru" ? links.desktopGuideRu : links.desktopGuide;
    $: mobilePaper = currentLocale === "ru" ? links.mobileGuideRu : links.mobileGuide;
    $: privacyPaper = currentLocale === "ru" ? links.privacyRu : links.privacy;

    let osHint: "desktop" | "mobile" | "other" = "other";
    let latestRelease = "";
    let latestReleaseDate = "";
    let latestReleaseUrl = links.releases;
    let mobileMenuOpen = false;

    const selectLocale = (next: AppLocale) => {
        currentLocale = next;
        mobileMenuOpen = false;
        setLocale(next);
        const url = new URL(window.location.href);
        url.searchParams.set("lang", next);
        replaceState(
            `${url.pathname}${url.search}${url.hash}`,
            {},
        );
    };

    onMount(() => {
        if (data.requestedLocale) {
            setLocale(data.requestedLocale);
        } else {
            currentLocale = $locale;
        }

        const ua = navigator.userAgent.toLowerCase();
        osHint = /(android|iphone|ipad|ipod)/.test(ua)
            ? "mobile"
            : /(mac os|win|linux)/.test(ua)
              ? "desktop"
              : "other";

        void fetch("https://api.github.com/repos/Kamori-app/kamori-app/releases/latest")
            .then((response) => (response.ok ? response.json() : null))
            .then((data: { tag_name?: string; html_url?: string; published_at?: string } | null) => {
                if (!data) return;
                latestRelease = data.tag_name?.trim() ?? "";
                latestReleaseUrl = data.html_url?.trim() || links.releases;
                latestReleaseDate = data.published_at?.slice(0, 10) ?? "";
            })
            .catch(() => undefined);

        const closeOnEscape = (event: KeyboardEvent) => {
            if (event.key === "Escape") mobileMenuOpen = false;
        };
        const closeOnDesktop = () => {
            if (window.innerWidth > 980) mobileMenuOpen = false;
        };
        window.addEventListener("keydown", closeOnEscape);
        window.addEventListener("resize", closeOnDesktop);
        return () => {
            window.removeEventListener("keydown", closeOnEscape);
            window.removeEventListener("resize", closeOnDesktop);
        };
    });
</script>

<svelte:head>
    <title>Kamori — encrypted organizer</title>
    <meta
        name="description"
        content="Offline-first encrypted calendars, tasks and contacts for web, desktop, Android and iOS."
    />
</svelte:head>

<a class="skip-link" href="#content">{c.skip}</a>

<div class="header-shell">
    <header class="site-header">
        <a class="brand" href="/" aria-label="Kamori home" on:click={() => (mobileMenuOpen = false)}>
            <BrandMark size={38} />
            <span>KAMORI</span>
        </a>
        <nav class="primary-nav" aria-label="Primary navigation">
            {#each c.nav as item}
                <a href={`#${item[0]}`}>{item[1]}</a>
            {/each}
        </nav>
        <div class="header-actions">
            <LocaleSwitch value={currentLocale} onSelect={selectLocale} />
            <a class="text-link" href={webAppUrl}>{c.openApp} →</a>
        </div>
        <button
            class:open={mobileMenuOpen}
            class="menu-toggle"
            type="button"
            aria-controls="mobile-navigation"
            aria-expanded={mobileMenuOpen}
            aria-label={mobileMenuOpen ? c.closeMenu : c.openMenu}
            on:click={() => (mobileMenuOpen = !mobileMenuOpen)}
        >
            <span></span><span></span>
        </button>
    </header>
    <div
        id="mobile-navigation"
        class:open={mobileMenuOpen}
        class="mobile-menu"
        aria-hidden={!mobileMenuOpen}
    >
        <nav aria-label="Mobile navigation">
            {#each c.nav as item, index}
                <a href={`#${item[0]}`} on:click={() => (mobileMenuOpen = false)}>
                    <span>0{index + 1}</span>{item[1]}
                </a>
            {/each}
        </nav>
        <div class="mobile-menu-actions">
            <LocaleSwitch value={currentLocale} onSelect={selectLocale} />
            <a href={webAppUrl} on:click={() => (mobileMenuOpen = false)}>{c.openApp} →</a>
        </div>
    </div>
</div>

<main id="content">
    <section class="hero animate-fade-slide">
        <div class="hero-copy">
            <p class="eyebrow"><span>{c.alpha}</span>{c.eyebrow}</p>
            <h1>{c.hero}</h1>
            <p class="lede">{c.intro}</p>
            <div class="hero-actions">
                <a class="button primary" href={`${webAppUrl}?start=signup`}>{c.start}</a>
                <a class="button secondary" href="#apps">{c.seeApps}</a>
            </div>
            <p class="availability">{c.available}</p>
        </div>

        <div class="hero-object" aria-hidden="true">
            <div class="folio">
                <div class="folio-top">
                    <BrandMark size={34} />
                    <span>LOCAL / SEALED</span>
                </div>
                <div class="folio-date">22 · 08</div>
                <div class="folio-line wide"></div>
                <div class="folio-line"></div>
                <div class="folio-entry coral"><b>09:30</b><span>Design review</span></div>
                <div class="folio-entry sun"><b>14:00</b><span>Project notes</span></div>
                <div class="folio-entry leaf"><b>18:20</b><span>Call Mira</span></div>
                <div class="folio-stamp">E2EE</div>
            </div>
            <div class="object-caption"><b>{c.noteTitle}</b><span>{c.noteBody}</span></div>
        </div>
    </section>

    <section class="numbered-section" id="product">
        <div class="section-index">{c.productLabel}</div>
        <div class="section-copy">
            <h2>{c.productTitle}</h2>
            <p class="section-lede">{c.productBody}</p>
        </div>
        <div class="principles">
            {#each c.productItems as item, index}
                <article>
                    <span>0{index + 1}</span>
                    <h3>{item[0]}</h3>
                    <p>{item[1]}</p>
                </article>
            {/each}
        </div>
    </section>

    <section class="numbered-section path-section" id="how-it-works">
        <div class="section-index">{c.approachLabel}</div>
        <div class="section-copy"><h2>{c.approachTitle}</h2></div>
        <ol class="data-path">
            {#each c.paths as item, index}
                <li>
                    <span>{index + 1}</span>
                    <div><b>{item[0]}</b><small>{item[1]}</small></div>
                </li>
            {/each}
        </ol>
        <div class="bridge-note">
            <span>{c.bridgeAside}</span>
            <code>{c.bridgePath}</code>
        </div>
        <a class="document-link" href={links.architecture} target="_blank" rel="noreferrer">
            {c.architecture} ↗
        </a>
    </section>

    <section class="numbered-section" id="apps">
        <div class="section-index">{c.downloadsLabel}</div>
        <div class="section-copy">
            <h2>{c.downloadsTitle}</h2>
            <p class="section-lede">{c.downloadsBody}</p>
        </div>
        <div class="app-ledger">
            <article>
                <div class="app-glyph web"><span></span><span></span><span></span></div>
                <p class="app-platform">WEB</p>
                <h3>{c.webTitle}</h3>
                <p>{c.webBody}</p>
                <a href={`${webAppUrl}?start=signup`}>{c.webAction} →</a>
            </article>
            <article class:recommended={osHint === "desktop"}>
                <div class="app-glyph desktop"><span></span></div>
                <p class="app-platform">MAC · WINDOWS · LINUX</p>
                <h3>{c.desktopTitle}</h3>
                <p>{c.desktopBody}</p>
                <a href={links.releases} target="_blank" rel="noreferrer">{c.desktopAction} →</a>
            </article>
            <article class:recommended={osHint === "mobile"}>
                <div class="app-glyph mobile"><span></span></div>
                <p class="app-platform">ANDROID · IOS</p>
                <h3>{c.mobileTitle}</h3>
                <p>{c.mobileBody}</p>
                <a href={links.releases} target="_blank" rel="noreferrer">{c.mobileAction} →</a>
            </article>
        </div>
        {#if latestRelease}
            <p class="release-line">
                {c.releasePrefix}: <b>{latestRelease}</b>{latestReleaseDate ? ` · ${latestReleaseDate}` : ""}
                <a href={latestReleaseUrl} target="_blank" rel="noreferrer">{c.releaseNotes} ↗</a>
            </p>
        {/if}
        <div class="document-row">
            <a href={desktopPaper} target="_blank" rel="noreferrer">{c.desktopPaper} ↗</a>
            <a href={mobilePaper} target="_blank" rel="noreferrer">{c.mobilePaper} ↗</a>
            <a href={links.buildDesktop} target="_blank" rel="noreferrer">{c.buildSource} ↗</a>
        </div>
    </section>

    <section class="numbered-section security-section" id="security">
        <div class="section-index">{c.securityLabel}</div>
        <div class="section-copy">
            <h2>{c.securityTitle}</h2>
            <p class="section-lede">{c.securityBody}</p>
        </div>
        <div class="security-docs">
            <a href={privacyPaper} target="_blank" rel="noreferrer"><span>01</span>{c.securityLinks[0]} ↗</a>
            <a href={links.architecture} target="_blank" rel="noreferrer"><span>02</span>{c.securityLinks[1]} ↗</a>
            <a href={links.security} target="_blank" rel="noreferrer"><span>03</span>{c.securityLinks[2]} ↗</a>
            <a href={links.roadmap} target="_blank" rel="noreferrer"><span>04</span>{c.securityLinks[3]} ↗</a>
        </div>
        <aside class="sharing-callout">
            <h3>{c.sharingTitle}</h3>
            <p>{c.sharingBody}</p>
        </aside>
    </section>

    <section class="numbered-section" id="questions">
        <div class="section-index">{c.questionsLabel}</div>
        <div class="section-copy"><h2>{c.questionsTitle}</h2></div>
        <div class="faq-list">
            {#each c.faq as item, index}
                <details open={index === 0}>
                    <summary><span>0{index + 1}</span>{item[0]}</summary>
                    <p>{item[1]}</p>
                    {#if index === 3}
                        <a href={links.selfHost} target="_blank" rel="noreferrer">Server documentation ↗</a>
                    {/if}
                </details>
            {/each}
        </div>
    </section>
</main>

<footer>
    <div class="brand"><BrandMark size={32} /><span>KAMORI</span></div>
    <p>{c.footer}</p>
    <div>
        <a href={repository} target="_blank" rel="noreferrer">{c.source}</a>
        <a href={links.license} target="_blank" rel="noreferrer">{c.license}</a>
    </div>
</footer>

<style>
    :global(body) { overflow-x: hidden; }
    .skip-link { position: fixed; left: 1rem; top: -4rem; z-index: 100; background: var(--ink); color: white; padding: .75rem 1rem; }
    .skip-link:focus { top: 1rem; }
    .header-shell { position: sticky; z-index: 50; top: 0; width: 100%; border-bottom: 1px solid var(--rule); background: var(--paper); background: color-mix(in srgb, var(--paper) 94%, transparent); backdrop-filter: blur(12px); }
    .site-header { width: min(1320px, calc(100% - 3rem)); margin: 0 auto; min-height: 84px; display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; }
    .brand { display: inline-flex; align-items: center; gap: .7rem; color: var(--ink); font-weight: 720; letter-spacing: .17em; text-decoration: none; }
    .primary-nav { display: flex; gap: clamp(1rem, 2.3vw, 2.4rem); }
    .primary-nav a, .text-link { color: var(--ink); font-size: .82rem; text-decoration: none; }
    .primary-nav a:hover, .text-link:hover { color: var(--coral); }
    .header-actions { display: flex; justify-content: flex-end; align-items: center; gap: 1.35rem; }
    .menu-toggle, .mobile-menu { display: none; }
    main, footer { width: min(1320px, calc(100% - 3rem)); margin-inline: auto; }
    .hero { min-height: 720px; display: grid; grid-template-columns: minmax(0, 1.1fr) minmax(360px, .9fr); align-items: center; gap: clamp(3rem, 8vw, 8rem); padding: 5rem 0 6rem; border-bottom: 1px solid var(--rule); }
    .eyebrow, .section-index, .app-platform { font-size: .73rem; font-weight: 650; letter-spacing: .1em; text-transform: uppercase; }
    .eyebrow { display: flex; align-items: center; gap: .8rem; color: var(--ink-soft); }
    .eyebrow span { padding: .35rem .55rem; background: var(--sun); color: var(--ink); }
    h1, h2, h3, p { margin-top: 0; }
    h1 { max-width: 800px; margin: 1.6rem 0; font-family: var(--font-serif); font-size: clamp(3.6rem, 6.6vw, 6.9rem); font-weight: 400; letter-spacing: -.055em; line-height: .97; }
    .lede { max-width: 690px; color: var(--ink-soft); font-size: clamp(1.05rem, 1.6vw, 1.32rem); line-height: 1.65; }
    .hero-actions { display: flex; flex-wrap: wrap; gap: .8rem; margin-top: 2rem; }
    .button { display: inline-flex; align-items: center; justify-content: center; min-height: 48px; padding: 0 1.25rem; border: 1px solid var(--ink); color: var(--ink); font-size: .88rem; font-weight: 650; text-decoration: none; }
    .button.primary { background: var(--ink); color: var(--paper); }
    .button:hover { transform: translate(-2px, -2px); box-shadow: 4px 4px 0 var(--coral); }
    .availability { margin: 1.1rem 0 0; color: var(--ink-soft); font-size: .78rem; letter-spacing: .05em; }
    .hero-object { position: relative; min-height: 560px; display: grid; place-items: center; }
    .hero-object::before { content: ""; position: absolute; width: 78%; aspect-ratio: 1; border-radius: 50%; background: var(--sun); opacity: .62; filter: blur(1px); }
    .folio { position: relative; z-index: 1; width: min(390px, 82%); min-height: 490px; box-sizing: border-box; padding: 1.5rem; border: 2px solid var(--ink); background: #faf7ed; box-shadow: 18px 18px 0 var(--ink); transform: rotate(2.2deg); }
    .folio-top { display: flex; justify-content: space-between; align-items: center; padding-bottom: 1rem; border-bottom: 1px solid var(--rule); font-size: .67rem; font-weight: 650; letter-spacing: .12em; }
    .folio-date { margin: 2rem 0 1.5rem; font-family: var(--font-serif); font-size: 4rem; }
    .folio-line { height: 8px; width: 62%; margin: .55rem 0; background: var(--paper-deep); }
    .folio-line.wide { width: 88%; }
    .folio-entry { display: grid; grid-template-columns: 68px 1fr; gap: .7rem; margin-top: 1rem; padding: .8rem; border-left: 8px solid; font-size: .83rem; }
    .folio-entry.coral { border-color: var(--coral); background: #f8ddd4; }
    .folio-entry.sun { border-color: var(--sun); background: #f8e9bf; }
    .folio-entry.leaf { border-color: var(--leaf); background: #dbe9df; }
    .folio-stamp { position: absolute; right: 1.2rem; bottom: 1.2rem; padding: .45rem; border: 2px solid var(--coral); color: var(--coral); font-weight: 700; letter-spacing: .12em; transform: rotate(-8deg); }
    .object-caption { position: absolute; z-index: 2; right: -1rem; bottom: 0; width: min(340px, 75%); padding: 1.1rem; background: var(--coral); color: #251f1b; transform: rotate(-1deg); }
    .object-caption b, .object-caption span { display: block; }
    .object-caption span { margin-top: .45rem; font-size: .78rem; line-height: 1.45; }
    .numbered-section { display: grid; grid-template-columns: minmax(130px, .3fr) minmax(0, 1.7fr); column-gap: clamp(2rem, 7vw, 7rem); padding: 7rem 0; border-bottom: 1px solid var(--rule); scroll-margin-top: 104px; }
    .section-index { color: var(--coral); padding-top: .55rem; }
    .section-copy h2 { max-width: 900px; margin-bottom: 1.5rem; font-family: var(--font-serif); font-size: clamp(2.7rem, 5vw, 5.1rem); font-weight: 400; letter-spacing: -.045em; line-height: 1.02; }
    .section-lede { max-width: 780px; color: var(--ink-soft); font-size: 1.08rem; line-height: 1.65; }
    .principles, .app-ledger, .security-docs, .faq-list, .data-path, .bridge-note, .document-link, .document-row, .release-line, .sharing-callout { grid-column: 2; }
    .principles { display: grid; grid-template-columns: repeat(3, 1fr); margin-top: 4rem; border-top: 1px solid var(--ink); }
    .principles article { padding: 1.4rem 1.4rem 0 0; border-right: 1px solid var(--rule); }
    .principles article + article { padding-left: 1.4rem; }
    .principles span, .security-docs span { color: var(--coral); font-size: .7rem; letter-spacing: .1em; }
    .principles h3, .app-ledger h3, .sharing-callout h3 { margin: 2.4rem 0 .8rem; font-family: var(--font-serif); font-size: 1.5rem; font-weight: 600; }
    .principles p, .app-ledger p, .sharing-callout p { color: var(--ink-soft); font-size: .9rem; line-height: 1.6; }
    .data-path { display: grid; grid-template-columns: repeat(4, 1fr); margin: 4rem 0 0; padding: 0; list-style: none; }
    .data-path li { position: relative; min-height: 145px; padding: 1rem 1.2rem; border: 1px solid var(--ink); border-right: 0; }
    .data-path li:last-child { border-right: 1px solid var(--ink); }
    .data-path li:not(:last-child)::after { content: "→"; position: absolute; z-index: 2; right: -.7rem; top: 50%; padding: .2rem; background: var(--paper); }
    .data-path li > span { color: var(--coral); font-size: .75rem; }
    .data-path b, .data-path small { display: block; }
    .data-path b { margin-top: 1.8rem; }
    .data-path small { margin-top: .4rem; color: var(--ink-soft); line-height: 1.4; }
    .bridge-note { display: grid; grid-template-columns: 220px 1fr; gap: 1rem; margin-top: 1rem; padding: 1rem 1.2rem; background: var(--ink); color: var(--paper); }
    .bridge-note span { color: var(--sun); font-size: .72rem; font-weight: 650; letter-spacing: .09em; text-transform: uppercase; }
    .bridge-note code { white-space: normal; font-family: var(--font-sans); font-size: .83rem; }
    .document-link { width: fit-content; margin-top: 1.3rem; color: var(--ink); font-size: .83rem; }
    .app-ledger { display: grid; grid-template-columns: repeat(3, 1fr); margin-top: 4rem; border: 1px solid var(--ink); }
    .app-ledger article { position: relative; min-height: 400px; padding: 1.5rem; border-right: 1px solid var(--ink); background: rgba(250, 247, 237, .5); }
    .app-ledger article:last-child { border-right: 0; }
    .app-ledger article.recommended::before { content: "●"; position: absolute; right: 1rem; top: 1rem; color: var(--coral); }
    .app-glyph { height: 130px; display: grid; place-items: center; margin-bottom: 1.5rem; background: var(--paper-deep); }
    .app-glyph.web { grid-template-columns: repeat(3, 34px); gap: 6px; }
    .app-glyph.web span { width: 34px; height: 52px; border: 2px solid var(--ink); }
    .app-glyph.web span:nth-child(2) { transform: translateY(-12px); background: var(--sun); }
    .app-glyph.desktop span { width: 108px; height: 70px; border: 3px solid var(--ink); box-shadow: 0 12px 0 -8px var(--ink); background: var(--coral); }
    .app-glyph.mobile span { width: 54px; height: 92px; border: 3px solid var(--ink); border-radius: 8px; background: var(--leaf); box-shadow: inset 0 -12px 0 var(--sun); }
    .app-platform { color: var(--coral) !important; }
    .app-ledger h3 { margin-top: 1.4rem; }
    .app-ledger a { position: absolute; left: 1.5rem; bottom: 1.5rem; color: var(--ink); font-size: .85rem; font-weight: 650; }
    .release-line { margin: 1.5rem 0 0; font-size: .8rem; }
    .release-line a { margin-left: .8rem; color: var(--ink); }
    .document-row { display: flex; flex-wrap: wrap; gap: 1rem 2rem; margin-top: 1.2rem; }
    .document-row a { color: var(--ink-soft); font-size: .78rem; }
    .security-docs { display: grid; grid-template-columns: repeat(2, 1fr); margin-top: 4rem; border-top: 1px solid var(--ink); }
    .security-docs a { display: flex; align-items: center; gap: 1rem; padding: 1.2rem 0; border-bottom: 1px solid var(--rule); color: var(--ink); text-decoration: none; }
    .security-docs a:nth-child(odd) { padding-right: 1.5rem; border-right: 1px solid var(--rule); }
    .security-docs a:nth-child(even) { padding-left: 1.5rem; }
    .sharing-callout { width: min(620px, 80%); margin: 4rem 0 0 auto; padding: 2rem; box-sizing: border-box; background: var(--sun); transform: rotate(-1deg); }
    .sharing-callout h3 { margin-top: 0; }
    .faq-list { margin-top: 3rem; }
    details { border-top: 1px solid var(--ink); }
    details:last-child { border-bottom: 1px solid var(--ink); }
    summary { display: flex; gap: 1.5rem; padding: 1.25rem 0; cursor: pointer; font-family: var(--font-serif); font-size: 1.22rem; font-weight: 600; list-style: none; }
    summary::-webkit-details-marker { display: none; }
    summary::after { content: "+"; margin-left: auto; font-family: var(--font-sans); }
    details[open] summary::after { content: "−"; }
    summary span { color: var(--coral); font-family: var(--font-sans); font-size: .7rem; }
    details > p, details > a { max-width: 760px; margin-left: 2.7rem; color: var(--ink-soft); font-size: .9rem; line-height: 1.65; }
    details > a { display: inline-block; margin-bottom: 1rem; }
    footer { display: grid; grid-template-columns: .6fr 1.2fr .6fr; align-items: center; gap: 2rem; padding: 2rem 0 3rem; }
    footer p { margin: 0; color: var(--ink-soft); font-size: .75rem; line-height: 1.5; }
    footer > div:last-child { display: flex; justify-content: flex-end; gap: 1rem; }
    footer a { color: var(--ink); font-size: .75rem; }

    @media (max-width: 980px) {
        .site-header { grid-template-columns: 1fr auto; }
        .primary-nav, .header-actions { display: none; }
        .menu-toggle { display: grid; width: 46px; height: 46px; padding: 0; place-content: center; gap: 7px; border: 1px solid var(--ink); background: transparent; color: var(--ink); cursor: pointer; }
        .menu-toggle span { display: block; width: 21px; height: 2px; background: currentColor; transition: transform .18s ease; }
        .menu-toggle.open span:first-child { transform: translateY(4.5px) rotate(45deg); }
        .menu-toggle.open span:last-child { transform: translateY(-4.5px) rotate(-45deg); }
        .mobile-menu { width: min(1320px, calc(100% - 3rem)); margin: 0 auto; padding: .4rem 0 1.25rem; }
        .mobile-menu.open { display: block; }
        .mobile-menu nav { display: grid; }
        .mobile-menu nav a { display: flex; align-items: center; gap: 1rem; padding: 1rem 0; border-bottom: 1px solid var(--rule); color: var(--ink); font-family: var(--font-serif); font-size: clamp(1.25rem, 4vw, 1.7rem); text-decoration: none; }
        .mobile-menu nav a span { color: var(--coral); font-family: var(--font-sans); font-size: .65rem; letter-spacing: .1em; }
        .mobile-menu-actions { display: flex; align-items: center; justify-content: space-between; gap: 1rem; padding-top: 1.2rem; }
        .mobile-menu-actions > a { color: var(--ink); font-size: .82rem; font-weight: 650; text-decoration: none; }
        .hero { grid-template-columns: 1fr; padding-top: 4rem; }
        .hero-object { min-height: 520px; }
        .numbered-section { grid-template-columns: 110px 1fr; column-gap: 2rem; }
        .data-path { grid-template-columns: repeat(2, 1fr); }
        .data-path li:nth-child(2) { border-right: 1px solid var(--ink); }
        .data-path li:nth-child(2)::after { display: none; }
        .app-ledger { grid-template-columns: 1fr; }
        .app-ledger article { min-height: 350px; border-right: 0; border-bottom: 1px solid var(--ink); }
        .app-ledger article:last-child { border-bottom: 0; }
    }

    @media (max-width: 680px) {
        .site-header, .mobile-menu, main, footer { width: min(100% - 2rem, 1320px); }
        .site-header { min-height: 70px; }
        .hero { min-height: auto; padding: 3.5rem 0 4rem; gap: 2rem; }
        h1 { font-size: clamp(3rem, 15vw, 4.8rem); }
        .hero-object { min-height: 480px; }
        .folio { width: 82%; min-height: 420px; box-shadow: 10px 10px 0 var(--ink); }
        .folio-date { font-size: 3rem; }
        .object-caption { right: 0; }
        .numbered-section { display: block; padding: 4.5rem 0; }
        .numbered-section { scroll-margin-top: 86px; }
        .section-index { margin-bottom: 2rem; }
        .section-copy h2 { font-size: clamp(2.5rem, 12vw, 4rem); }
        .principles, .security-docs { grid-template-columns: 1fr; }
        .principles article, .principles article + article { padding: 1.2rem 0; border-right: 0; border-bottom: 1px solid var(--rule); }
        .principles h3 { margin-top: 1rem; }
        .data-path { grid-template-columns: 1fr; }
        .data-path li { border-right: 1px solid var(--ink); border-bottom: 0; }
        .data-path li:last-child { border-bottom: 1px solid var(--ink); }
        .data-path li::after { display: none; }
        .bridge-note { grid-template-columns: 1fr; }
        .security-docs a, .security-docs a:nth-child(odd), .security-docs a:nth-child(even) { padding: 1rem 0; border-right: 0; }
        .sharing-callout { width: 94%; }
        footer { grid-template-columns: 1fr; }
        footer > div:last-child { justify-content: flex-start; }
    }
</style>
