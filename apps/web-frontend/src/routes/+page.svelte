<script lang="ts">
    import { onMount } from "svelte";
    import Button from "$lib/components/ui/Button.svelte";
    import {
        computeScrollTarget,
        computeTopOffset,
        pickActiveSection,
    } from "$lib/nav/section-nav";

    /**
     * Public landing page with sticky section navigation.
     * Keeps active tab in sync with scroll position instead of click state.
     */
    const sourceUrl = "https://github.com/Kamori-app/kamori-app";
    const releaseUrl = `${sourceUrl}/releases`;
    const issuesUrl = `${sourceUrl}/issues`;
    const selfHostGuideUrl = `${sourceUrl}/tree/main/apps/cloud-server`;
    const commercialLicensingUrl = `mailto:contact@kamori.app`;
    const latestReleaseApi =
        "https://api.github.com/repos/Kamori-app/kamori-app/releases/latest";

    const navItems = [
        { id: "why-kamori", label: "Why", shortLabel: "Why" },
        { id: "what-kamori-is", label: "What it is", shortLabel: "What" },
        { id: "how-it-works", label: "How it works", shortLabel: "How" },
        { id: "downloads", label: "Downloads", shortLabel: "DL" },
        { id: "compatibility", label: "Compatibility", shortLabel: "Compat" },
        { id: "security", label: "Security", shortLabel: "Sec" },
        { id: "sharing", label: "Sharing", shortLabel: "Share" },
        { id: "faq", label: "FAQ", shortLabel: "FAQ" },
    ];

    let activeSection = "why-kamori";
    let showStickyNav = false;
    let stickyNavEl: HTMLElement | null = null;
    let mobileNavOpen = false;
    let osHint: "macos" | "windows" | "linux" | "ios" | "android" | "other" =
        "other";
    let latestReleaseLabel = "";
    let latestReleaseDate = "";
    let latestReleaseHref = releaseUrl;

    /**
     * Smooth-scroll handler for section anchors with sticky-nav offset correction.
     */
    const scrollToSection = (event: MouseEvent, id: string) => {
        event.preventDefault();
        const section = document.getElementById(id);
        if (!section) {
            return;
        }
        const navHeight = stickyNavEl?.getBoundingClientRect().height ?? 0;
        const sectionAbsTop =
            section.getBoundingClientRect().top + window.scrollY;
        const targetTop = computeScrollTarget(sectionAbsTop, navHeight, 20);
        window.history.replaceState(null, "", `#${id}`);
        window.scrollTo({ top: targetTop, behavior: "smooth" });
        mobileNavOpen = false;
    };

    onMount(() => {
        // Lightweight OS hint is used to highlight likely download action.
        const ua = navigator.userAgent.toLowerCase();
        if (ua.includes("android")) {
            osHint = "android";
        } else if (/(iphone|ipad|ipod)/.test(ua)) {
            osHint = "ios";
        } else if (ua.includes("mac os")) {
            osHint = "macos";
        } else if (ua.includes("win")) {
            osHint = "windows";
        } else if (ua.includes("linux")) {
            osHint = "linux";
        }

        /**
         * Computes active section from current scroll anchor point.
         */
        const updateActiveSection = () => {
            const navHeight = stickyNavEl?.getBoundingClientRect().height ?? 0;
            const topOffset = computeTopOffset(navHeight);

            const sections = navItems
                .map((item) => {
                    const section = document.getElementById(item.id);
                    if (!section) {
                        return null;
                    }
                    return {
                        id: item.id,
                        absTop:
                            section.getBoundingClientRect().top +
                            window.scrollY,
                    };
                })
                .filter(
                    (entry): entry is { id: string; absTop: number } =>
                        entry !== null,
                );

            const current = pickActiveSection(
                sections,
                window.scrollY,
                topOffset,
            );
            if (current) {
                activeSection = current;
            }
        };

        const handleScroll = () => {
            showStickyNav = window.scrollY > 120;
            updateActiveSection();
        };

        const handleResize = () => {
            if (window.innerWidth > 840) {
                mobileNavOpen = false;
            }
            handleScroll();
        };

        /**
         * Fetches latest GitHub release metadata for trust signals in download section.
         */
        const loadLatestRelease = async () => {
            try {
                const response = await fetch(latestReleaseApi);
                if (!response.ok) {
                    latestReleaseLabel = "";
                    return;
                }
                const data = (await response.json()) as {
                    tag_name?: string;
                    html_url?: string;
                    published_at?: string;
                };
                latestReleaseLabel = data.tag_name?.trim() || "";
                latestReleaseHref = data.html_url?.trim() || releaseUrl;
                if (data.published_at) {
                    latestReleaseDate = data.published_at.slice(0, 10);
                }
            } catch {
                latestReleaseLabel = "";
            }
        };

        handleScroll();
        void loadLatestRelease();
        window.addEventListener("scroll", handleScroll, { passive: true });
        window.addEventListener("resize", handleResize, { passive: true });

        return () => {
            window.removeEventListener("scroll", handleScroll);
            window.removeEventListener("resize", handleResize);
        };
    });
</script>

<main class="min-h-screen px-4 py-10 md:px-8">
    <section
        class="mx-auto max-w-6xl animate-fade-slide rounded-3xl border border-white/60 bg-white/65 p-8 shadow-panel backdrop-blur-sm"
    >
        <div class="grid gap-10">
            <div class="max-w-4xl">
                <div
                    class="flex flex-wrap items-center gap-x-2 gap-y-1 whitespace-normal text-[11px] font-semibold uppercase tracking-[0.12em] text-slate/60 md:flex-nowrap md:whitespace-nowrap md:tracking-[0.16em] md:text-xs"
                >
                    <span
                        class="shrink-0 font-heading text-sm font-bold tracking-[0.18em] text-slate"
                    >
                        KAMORI
                    </span>
                    <span>Secure sync with end-to-end encryption</span>
                    <span
                        class="rounded-full border border-slate/15 bg-sand/60 px-2 py-1 normal-case tracking-normal text-slate/80"
                    >
                        Alpha
                    </span>
                    <span
                        class="rounded-full border border-slate/15 bg-white/70 px-2 py-1 normal-case tracking-normal text-slate/75"
                    >
                        <a
                            class="ml-1 underline underline-offset-2 hover:text-slate"
                            href={sourceUrl}
                            target="_blank"
                            rel="noreferrer"
                        >
                            View source
                        </a>
                    </span>
                </div>

                <div class="mt-4">
                    <h1
                        class="max-w-2xl font-heading text-4xl font-bold leading-tight text-slate md:text-5xl"
                    >
                        Private calendar & contacts sync — encrypted on your
                        devices.
                    </h1>
                </div>

                <p
                    class="mt-4 max-w-3xl text-sm leading-relaxed text-slate/80 md:text-base"
                >
                    Use Kamori directly on web, desktop, Android, and iOS. The
                    desktop app can also expose a local DAV bridge for existing
                    calendar and contacts apps. Keys never leave your devices —
                    the cloud syncs encrypted data only.
                </p>
                <div class="mt-4 flex flex-wrap gap-2 text-xs text-slate/80">
                    <span
                        class="rounded-full border border-slate/15 bg-white/70 px-3 py-1"
                        >CalDAV (calendars & reminders)</span
                    >
                    <span
                        class="rounded-full border border-slate/15 bg-white/70 px-3 py-1"
                        >CardDAV (contacts)</span
                    >
                    <span
                        class="rounded-full border border-slate/15 bg-white/70 px-3 py-1"
                    >
                        macOS • Windows • Linux • iOS • Android • Web
                    </span>
                </div>

                <div class="mt-7 flex flex-wrap items-center gap-3">
                    <a href="/app?start=signup"
                        ><Button>Sign up on Kamori Cloud</Button></a
                    >
                    <a href="#downloads"
                        ><Button variant="secondary"
                            >Download Bridge Apps</Button
                        ></a
                    >
                </div>
            </div>
        </div>
    </section>

    <nav
        bind:this={stickyNavEl}
        class={`sticky top-3 z-30 mx-auto mt-6 max-w-6xl rounded-2xl border border-white/70 bg-white/80 p-2 shadow-panel backdrop-blur-sm transition-all duration-200 max-[870px]:p-1.5 ${
            showStickyNav
                ? "opacity-100 translate-y-0"
                : "pointer-events-none opacity-0 -translate-y-2"
        }`}
    >
        <div class="flex items-center justify-between gap-2">
            <span
                class="shrink-0 font-heading text-sm font-bold tracking-[0.18em] text-slate"
            >
                KAMORI
            </span>
            <button
                type="button"
                class="ml-auto hidden rounded-lg border border-slate/20 bg-white/80 px-3 py-1.5 text-xs font-semibold text-slate max-[840px]:inline-flex"
                aria-expanded={mobileNavOpen}
                aria-controls="mobile-sections-menu"
                on:click={() => (mobileNavOpen = !mobileNavOpen)}
            >
                Sections
            </button>
            <ul
                class="flex min-w-max items-center gap-2 text-sm max-[870px]:gap-1 max-[870px]:text-xs max-[840px]:hidden"
            >
                {#each navItems as item}
                    <li>
                        <a
                            class={`inline-flex rounded-lg px-3 py-2 max-[870px]:px-2 max-[870px]:py-1.5 max-[580px]:px-1.5 max-[580px]:py-1 ${
                                activeSection === item.id
                                    ? "bg-slate text-white"
                                    : "text-slate/80 hover:bg-surface hover:text-slate"
                            }`}
                            href={`#${item.id}`}
                            on:click={(event) =>
                                scrollToSection(event, item.id)}
                        >
                            {item.label}
                        </a>
                    </li>
                {/each}
            </ul>
        </div>
        {#if mobileNavOpen}
            <ul
                id="mobile-sections-menu"
                class="mt-2 hidden grid-cols-1 gap-1 rounded-xl border border-slate/15 bg-white/90 p-1 max-[840px]:grid"
            >
                {#each navItems as item}
                    <li>
                        <a
                            class={`block rounded-lg px-3 py-2 text-sm ${
                                activeSection === item.id
                                    ? "bg-slate text-white"
                                    : "text-slate/80 hover:bg-surface hover:text-slate"
                            }`}
                            href={`#${item.id}`}
                            on:click={(event) =>
                                scrollToSection(event, item.id)}
                        >
                            {item.label}
                        </a>
                    </li>
                {/each}
            </ul>
        {/if}
    </nav>

    <section
        id="why-kamori"
        class="mx-auto mt-8 max-w-6xl scroll-mt-32 rounded-2xl border border-white/70 bg-white/80 p-6 shadow-panel"
    >
        <h2 class="font-heading text-2xl font-semibold text-slate">
            Why Kamori
        </h2>
        <p
            class="mt-3 max-w-4xl text-sm leading-relaxed text-slate/85 md:text-base"
        >
            Because most cloud sync is readable on servers — Kamori is built for
            private-by-default sync.
        </p>
        <div class="mt-5 grid gap-4 md:grid-cols-3">
            <article
                class="rounded-2xl border border-white/70 bg-surface/60 p-4"
            >
                <h3 class="text-base font-semibold text-slate md:text-lg">
                    Not readable on the server
                </h3>
                <p class="mt-2 text-sm text-slate/85">
                    Most calendar/contact sync services store data in a form the
                    service can read. Kamori keeps content encrypted on your
                    devices and syncs only encrypted data through the cloud.
                </p>
            </article>
            <article
                class="rounded-2xl border border-white/70 bg-surface/60 p-4"
            >
                <h3 class="text-base font-semibold text-slate md:text-lg">
                    Reduce the impact of a breach
                </h3>
                <p class="mt-2 text-sm text-slate/85">
                    If a cloud account is compromised or a server is breached,
                    server-readable data becomes a single point of failure. With
                    Kamori, keys stay on your devices, so server-side data is
                    far less useful.
                </p>
            </article>
            <article
                class="rounded-2xl border border-white/70 bg-surface/60 p-4"
            >
                <h3 class="text-base font-semibold text-slate md:text-lg">
                    Keep your existing apps
                </h3>
                <p class="mt-2 text-sm text-slate/85">
                    On desktop, Kamori can expose a local CalDAV/CardDAV bridge
                    so you can keep using familiar calendar and contacts apps.
                    On mobile, optional system projection integrates with the
                    operating system without running a localhost server.
                </p>
            </article>
        </div>
    </section>

    <section
        id="what-kamori-is"
        class="mx-auto mt-8 max-w-6xl scroll-mt-32 rounded-2xl border border-white/70 bg-white/80 p-6 shadow-panel"
    >
        <h2 class="font-heading text-2xl font-semibold text-slate">
            What Kamori is
        </h2>
        <p
            class="mt-3 max-w-4xl text-sm leading-relaxed text-slate/85 md:text-base"
        >
            A private sync layer for calendars and contacts - designed to work
            with your existing apps.
        </p>
        <ul
            class="mt-4 list-disc space-y-1 pl-5 text-sm text-slate/85 md:text-base"
        >
            <li>Provides first-party web, desktop, Android, and iOS clients.</li>
            <li>
                Offers a local CalDAV/CardDAV bridge on desktop and optional
                system projection on mobile.
            </li>
            <li>
                The cloud syncs encrypted data — keys never leave your devices.
            </li>
        </ul>
        <details
            class="mt-4 rounded-2xl border border-white/70 bg-surface/70 p-4"
        >
            <summary class="cursor-pointer text-sm font-semibold text-slate"
                >Is it like Syncthing?</summary
            >
            <p class="mt-2 text-sm text-slate/85">
                Similar idea (sync across devices), but Kamori is built for
                calendars and contacts — not general file sync.
            </p>
        </details>
    </section>

    <section id="how-it-works" class="mx-auto mt-8 max-w-6xl scroll-mt-32">
        <h2 class="font-heading text-2xl font-semibold text-slate">
            How it works
        </h2>
        <p
            class="mt-3 max-w-4xl text-sm leading-relaxed text-slate/85 md:text-base"
        >
            Create an account in the web portal, run the bridge on your devices,
            and connect your DAV clients to the local endpoint.
        </p>
        <a
            href="#quickstart"
            class="mt-3 inline-block text-sm font-semibold text-slate/75 underline underline-offset-2 hover:text-slate"
        >
            Go to Quickstart
        </a>
    </section>

    <section
        id="downloads"
        class="mx-auto mt-6 max-w-6xl scroll-mt-32 rounded-2xl border border-white/70 bg-white/80 p-6 shadow-panel"
    >
        <h2 class="font-heading text-2xl font-semibold text-slate">
            Downloads
        </h2>
        <p
            class="mt-3 max-w-4xl text-sm leading-relaxed text-slate/85 md:text-base"
        >
            Use the web portal to create an account. Install the bridge on each
            device for day-to-day sync.
        </p>

        <div class="mt-5 rounded-2xl border border-white/70 bg-surface/70 p-4">
            <p
                class="text-xs font-semibold uppercase tracking-wide text-slate/70"
            >
                Sync path
            </p>
            <p class="mt-2 text-sm text-slate/80">
                Encryption happens on your devices. The cloud stores and syncs
                ciphertext only.
            </p>
            <div
                class="mt-3 grid gap-2 text-center text-sm font-semibold text-slate md:grid-cols-5 md:items-center"
            >
                <div class="rounded-xl border border-slate/15 bg-white p-3">
                    DAV Apps
                    <p class="mt-1 text-xs font-normal text-slate/70">
                        (plaintext on device)
                    </p>
                </div>
                <div class="text-slate/50">↔</div>
                <div class="rounded-xl border border-slate/15 bg-white p-3">
                    Local Bridge
                    <p class="mt-1 text-xs font-normal text-slate/70">
                        (encrypt/decrypt)
                    </p>
                </div>
                <div class="text-slate/50">↔</div>
                <div class="rounded-xl border border-slate/15 bg-white p-3">
                    Encrypted Cloud
                    <p class="mt-1 text-xs font-normal text-slate/70">
                        (ciphertext)
                    </p>
                </div>
            </div>
        </div>

        <div class="mt-5 grid gap-4 md:grid-cols-2">
            <article
                class="rounded-2xl border border-white/70 bg-surface/60 p-4"
            >
                <p
                    class="text-xs font-semibold uppercase tracking-wide text-slate/70"
                >
                    Download
                </p>
                <div class="mt-3 space-y-3">
                    <div
                        class="rounded-xl border border-slate/15 bg-white/85 p-3"
                    >
                        <p class="text-sm font-semibold text-slate">
                            Desktop Bridge
                        </p>
                        <p class="mt-1 text-xs text-slate/75">
                            macOS • Windows • Linux
                        </p>
                        <div
                            class="mt-3 grid gap-2 sm:grid-cols-2 2xl:grid-cols-3"
                        >
                            <a
                                class="block"
                                href={releaseUrl}
                                target="_blank"
                                rel="noreferrer"
                                ><Button
                                    fullWidth
                                    variant={osHint === "macos"
                                        ? "primary"
                                        : "secondary"}
                                    >Download for macOS</Button
                                ></a
                            >
                            <a
                                class="block"
                                href={releaseUrl}
                                target="_blank"
                                rel="noreferrer"
                                ><Button
                                    fullWidth
                                    variant={osHint === "windows"
                                        ? "primary"
                                        : "secondary"}
                                    >Download for Windows</Button
                                ></a
                            >
                            <a
                                class="block"
                                href={releaseUrl}
                                target="_blank"
                                rel="noreferrer"
                                ><Button
                                    fullWidth
                                    variant={osHint === "linux"
                                        ? "primary"
                                        : "secondary"}
                                    >Download for Linux</Button
                                ></a
                            >
                        </div>
                    </div>
                    <div
                        class="rounded-xl border border-slate/15 bg-white/85 p-3"
                    >
                        <p class="text-sm font-semibold text-slate">
                            Mobile Bridge
                        </p>
                        <p class="mt-1 text-xs text-slate/75">iOS • Android</p>
                        <div class="mt-3 grid gap-2 sm:grid-cols-2">
                            <a
                                class="block"
                                href={releaseUrl}
                                target="_blank"
                                rel="noreferrer"
                                ><Button
                                    fullWidth
                                    variant={osHint === "ios"
                                        ? "primary"
                                        : "secondary"}>Download for iOS</Button
                                ></a
                            >
                            <a
                                class="block"
                                href={releaseUrl}
                                target="_blank"
                                rel="noreferrer"
                                ><Button
                                    fullWidth
                                    variant={osHint === "android"
                                        ? "primary"
                                        : "secondary"}
                                    >Download for Android</Button
                                ></a
                            >
                        </div>
                    </div>
                </div>
                {#if latestReleaseLabel}
                    <p class="mt-3 text-xs text-slate/65">
                        Latest release: {latestReleaseLabel}
                        {#if latestReleaseDate}
                            ({latestReleaseDate})
                        {/if}
                        —
                        <a
                            class="underline underline-offset-2 hover:text-slate/85"
                            href={latestReleaseHref}
                            target="_blank"
                            rel="noreferrer"
                        >
                            release notes
                        </a>
                    </p>
                {/if}
                <div
                    class="mt-3 flex flex-wrap items-center gap-3 text-xs text-slate/60"
                >
                    <a
                        class="underline underline-offset-2 hover:text-slate/80"
                        href={releaseUrl}
                        target="_blank"
                        rel="noreferrer"
                    >
                        Release artifacts
                    </a>
                    <a
                        class="underline underline-offset-2 hover:text-slate/80"
                        href={releaseUrl}
                        target="_blank"
                        rel="noreferrer"
                    >
                        Checksums / signatures
                    </a>
                    <a
                        class="underline underline-offset-2 hover:text-slate/80"
                        href={releaseUrl}
                        target="_blank"
                        rel="noreferrer"
                    >
                        Release notes
                    </a>
                    <a
                        class="underline underline-offset-2 hover:text-slate/80"
                        href={sourceUrl}
                        target="_blank"
                        rel="noreferrer"
                    >
                        Build from source
                    </a>
                </div>
            </article>

            <article
                id="quickstart"
                class="rounded-2xl border border-white/70 bg-surface/60 p-4 scroll-mt-32"
            >
                <p
                    class="text-xs font-semibold uppercase tracking-wide text-slate/70"
                >
                    Quickstart
                </p>
                <div
                    class="mt-3 rounded-xl border border-slate/15 bg-white/85 p-3"
                >
                    <ol
                        class="mt-2 list-decimal space-y-1 pl-5 text-sm text-slate/85"
                    >
                        <li>
                            Install bridge app and sign in with your Kamori
                            account.
                        </li>
                        <li>
                            Open bridge status screen and copy local endpoint
                            URL.
                        </li>
                        <li>
                            Add DAV account in your calendar/contacts app using
                            that endpoint.
                        </li>
                    </ol>
                    <p class="mt-2 text-xs text-slate/75">
                        Example endpoint: `http://127.0.0.1:8181`
                    </p>
                    <p class="mt-1 text-xs text-slate/75">
                        Platform notes: iOS/Android setup may differ - see
                        <a
                            class="underline underline-offset-2 hover:text-slate"
                            href="#compatibility">Compatibility</a
                        >.
                    </p>
                </div>
                <div
                    class="mt-3 rounded-xl border border-slate/15 bg-white/85 p-3"
                >
                    <p
                        class="text-xs font-semibold uppercase tracking-wide text-slate/70"
                    >
                        Supported clients (examples)
                    </p>
                    <p class="mt-2 text-sm text-slate/85">
                        Compatibility varies by client. We publish tested
                        clients per release in compatibility notes.
                    </p>
                </div>
            </article>
        </div>

        <details
            class="mt-4 rounded-2xl border border-white/70 bg-white/80 p-4"
        >
            <summary class="cursor-pointer text-sm font-semibold text-slate"
                >Conflict handling</summary
            >
            <p class="mt-2 text-sm text-slate/85">
                When edits happen concurrently, the bridge applies a
                deterministic last-write-wins policy (timestamp-based).
            </p>
        </details>
    </section>

    <section
        id="compatibility"
        class="mx-auto mt-8 max-w-6xl scroll-mt-32 rounded-2xl border border-white/70 bg-white/80 p-6 shadow-panel"
    >
        <h2 class="font-heading text-2xl font-semibold text-slate">
            Compatibility
        </h2>
        <ul class="mt-3 list-disc space-y-1 pl-5 text-sm text-slate/85">
            <li>
                <b>Tested clients:</b> list is published per release in compatibility
                notes.
            </li>
            <li>
                <b>Platform notes:</b> local endpoint setup can differ by client/platform.
            </li>
        </ul>
        <a
            class="mt-3 inline-block text-xs text-slate/70 underline underline-offset-2 hover:text-slate"
            href={releaseUrl}
            target="_blank"
            rel="noreferrer"
        >
            Open release and compatibility notes
        </a>
    </section>

    <section
        id="security"
        class="mx-auto mt-8 max-w-6xl scroll-mt-32 rounded-2xl border border-white/70 bg-white/80 p-6 shadow-panel"
    >
        <h2 class="font-heading text-2xl font-semibold text-slate">Security</h2>
        <p class="mt-2 text-sm text-slate/80">
            Trust artifacts:
            <a
                class="underline underline-offset-2 hover:text-slate"
                href={sourceUrl}
                target="_blank"
                rel="noreferrer">responsible disclosure</a
            >
            •
            <a
                class="underline underline-offset-2 hover:text-slate"
                href={releaseUrl}
                target="_blank"
                rel="noreferrer">checksums/signatures</a
            >
            •
            <a
                class="underline underline-offset-2 hover:text-slate"
                href={sourceUrl}
                target="_blank"
                rel="noreferrer">threat model/design notes</a
            >
        </p>
        <details
            class="mt-4 rounded-2xl border border-white/70 bg-white/80 p-4"
        >
            <summary class="cursor-pointer text-sm font-semibold text-slate"
                >Technical details</summary
            >
            <div class="mt-3 space-y-3 text-sm text-slate/85 md:text-base">
                <ul class="list-disc space-y-1 pl-5">
                    <li>
                        <b>Platform stack:</b> Rust core/runtime + Rust backend APIs
                        with TypeScript/Svelte and Flutter clients. This is security-oriented
                        because critical crypto and sync logic stays in one Rust core
                        (memory-safe by design), reducing implementation drift across
                        platforms.
                    </li>
                    <li>
                        <b>Payload encryption:</b> XChaCha20-Poly1305 with 24-byte
                        nonce (AES-256-GCM with 12-byte nonce is retained as backup
                        compatibility mode).
                    </li>
                    <li>
                        <b>Padding:</b> Attachments are also padded to 1 MB boundaries
                        before cloud upload, which reduces leakage of exact attachment
                        sizes.
                    </li>
                    <li>
                        <b>Key exchange and wrapping helpers:</b> X25519 + HKDF-SHA256.
                    </li>
                    <li>
                        <b>Password authentication:</b> OPAQUE (PAKE-class flow).
                    </li>
                    <li>
                        <b>Passwordless authentication:</b> passkeys via WebAuthn/FIDO2.
                    </li>
                    <li>
                        <b>Transport:</b> MessagePack with bytes-oriented binary fields.
                    </li>
                </ul>
                <p>
                    <b>Why OPAQUE + passkeys:</b> OPAQUE avoids sending raw passwords
                    and improves resistance to offline guessing after server-side
                    leaks; passkeys reduce phishing and password reuse by using device-bound
                    credentials.
                </p>
            </div>
        </details>
    </section>

    <section
        id="sharing"
        class="mx-auto mt-8 max-w-6xl scroll-mt-32 rounded-2xl border border-white/70 bg-white/80 p-6 shadow-panel"
    >
        <h2 class="font-heading text-2xl font-semibold text-slate">Sharing</h2>
        <ul
            class="mt-4 list-disc space-y-1 pl-5 text-sm text-slate/85 md:text-base"
        >
            <li>You choose invite expiry from 15 minutes to 7 days.</li>
            <li>Only registered accounts can redeem invite codes.</li>
            <li>Optional invite notes are encrypted client-side.</li>
        </ul>

        <details
            class="mt-4 rounded-2xl border border-white/70 bg-surface/70 p-4"
        >
            <summary class="cursor-pointer text-sm font-semibold text-slate"
                >Implementation details</summary
            >
            <ul class="mt-2 list-disc space-y-1 pl-5 text-sm text-slate/85">
                <li>Invite code is generated on the client.</li>
                <li>
                    Server stores only invite-code hash, encrypted collection
                    key, and optional encrypted note.
                </li>
                <li>
                    Redeem marks the code as used, so it cannot be redeemed
                    again.
                </li>
            </ul>
        </details>
    </section>

    <section
        id="faq"
        class="mx-auto mt-8 max-w-6xl rounded-2xl border border-white/70 bg-white/80 p-6 shadow-panel"
    >
        <h2 class="font-heading text-2xl font-semibold text-slate">FAQ</h2>
        <div class="mt-4 space-y-3">
            <details
                class="rounded-xl border border-slate/15 bg-surface/60 p-3"
            >
                <summary class="cursor-pointer text-sm font-semibold text-slate"
                    >Does the server see my data?</summary
                >
                <p class="mt-2 text-sm text-slate/85">
                    The cloud stores and syncs encrypted data. Keys stay on your
                    devices. Nobody can see it except you and people you
                    explicitly share your collection with via invite code (see
                    <a
                        class="underline underline-offset-2 hover:text-slate"
                        href="#sharing">Sharing</a
                    >).
                </p>
            </details>
            <details
                class="rounded-xl border border-slate/15 bg-surface/60 p-3"
            >
                <summary class="cursor-pointer text-sm font-semibold text-slate"
                    >What metadata is visible?</summary
                >
                <p class="mt-2 text-sm text-slate/85">
                    The server may see operational metadata (for example object
                    IDs, sizes, and sync timing). Content remains encrypted.
                </p>
            </details>
            <details
                class="rounded-xl border border-slate/15 bg-surface/60 p-3"
            >
                <summary class="cursor-pointer text-sm font-semibold text-slate"
                    >What happens if a device is lost?</summary
                >
                <p class="mt-2 text-sm text-slate/85">
                    Revoke that device&apos;s access from another signed-in
                    device and rotate keys for shared data.
                </p>
            </details>
            <details
                class="rounded-xl border border-slate/15 bg-surface/60 p-3"
            >
                <summary class="cursor-pointer text-sm font-semibold text-slate"
                    >Can I self-host Kamori?</summary
                >
                <p class="mt-2 text-sm text-slate/85">
                    The AGPL-3.0-only server and web client may be self-hosted,
                    including commercially, when you comply with the license
                    and offer the exact corresponding source to network users.
                </p>
                <p class="mt-2 text-sm text-slate/85">
                    A separate commercial license may be offered later for
                    organizations that need different terms; it does not remove
                    rights already granted by the AGPL release.
                </p>
                <p class="mt-2 text-sm text-slate/80">
                    <a
                        class="underline underline-offset-2 hover:text-slate"
                        href={selfHostGuideUrl}
                        target="_blank"
                        rel="noreferrer"
                    >
                        Self-host guide
                    </a>
                    •
                    <a
                        class="ml-1 underline underline-offset-2 hover:text-slate"
                        href={commercialLicensingUrl}
                        target="_blank"
                        rel="noreferrer"
                    >
                        Commercial licensing
                    </a>
                </p>
            </details>
            <details
                class="rounded-xl border border-slate/15 bg-surface/60 p-3"
            >
                <summary class="cursor-pointer text-sm font-semibold text-slate"
                    >How do you secure logins and encryption?</summary
                >
                <p class="mt-2 text-sm text-slate/85">
                    See <a
                        class="underline underline-offset-2 hover:text-slate"
                        href="#security">Security - Technical details</a
                    >
                    (encryption primitives, passkeys, password login).
                </p>
            </details>
        </div>
    </section>

    <footer class="mx-auto mt-8 max-w-6xl border-t border-slate/15 py-6 text-xs text-slate/70">
        <p>Kamori web is licensed under AGPL-3.0-only and comes without warranty.</p>
        <a class="underline underline-offset-2 hover:text-slate" href={sourceUrl} target="_blank" rel="noreferrer">
            View the corresponding source and license
        </a>
    </footer>
</main>
