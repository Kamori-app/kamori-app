<script lang="ts">
  import { adminApi, type AuditEntry, type Dashboard, type RuntimeSetting } from "$lib/api";
  import {
    parseCreationOptions,
    parseRequestOptions,
    serializeAssertion,
    serializeAttestation,
  } from "$lib/webauthn";

  const baseUrl =
    (import.meta.env.VITE_KAMORI_API_BASE_URL as string | undefined)?.trim() ||
    "http://127.0.0.1:3000";
  let username = "";
  let totpCode = "";
  let bootstrapToken = "";
  let sessionToken = "";
  let sessionExpiresAt = 0;
  let busy = "";
  let notice = "Operator credentials are memory-only and never stored by this page.";
  let dashboard: Dashboard | null = null;
  let settings: RuntimeSetting[] = [];
  let audit: AuditEntry[] = [];
  let drafts: Record<string, string> = {};
  let mutationTotp = "";
  let mutationReason = "";
  let settingConfirmation = "";
  let suspensionUserId = "";
  let suspensionEnabled = true;
  let suspensionConfirmation = "";
  let newSecurityKeyName = "Backup security key";
  let securityKeyConfirmation = "";
  let removeSecurityKeyConfirmation = "";

  const requireWebAuthn = () => {
    if (!("PublicKeyCredential" in window) || !navigator.credentials) {
      throw new Error("This browser cannot use WebAuthn security keys.");
    }
  };

  const errorMessage = (error: unknown) =>
    error instanceof Error ? error.message : String(error);

  const enroll = async () => {
    busy = "enroll";
    try {
      requireWebAuthn();
      const start = await adminApi.bootstrapStart(baseUrl, {
        username: username.trim(),
        bootstrap_token: bootstrapToken.trim(),
        totp_code: totpCode.trim(),
      });
      const credential = (await navigator.credentials.create({
        publicKey: parseCreationOptions(start.public_key_credential_creation_options),
      })) as PublicKeyCredential | null;
      if (!credential) throw new Error("Security-key enrollment was cancelled.");
      await adminApi.bootstrapFinish(baseUrl, {
        username: username.trim(),
        bootstrap_token: bootstrapToken.trim(),
        totp_code: totpCode.trim(),
        flow_id: start.flow_id,
        credential: serializeAttestation(credential),
      });
      bootstrapToken = "";
      totpCode = "";
      notice = "Operator enrolled. Sign in with the security key and TOTP.";
    } catch (error) {
      notice = `Enrollment failed: ${errorMessage(error)}`;
    } finally {
      busy = "";
    }
  };

  const getAssertion = async (options: Uint8Array): Promise<Uint8Array> => {
    requireWebAuthn();
    const credential = (await navigator.credentials.get({
      publicKey: parseRequestOptions(options),
    })) as PublicKeyCredential | null;
    if (!credential) throw new Error("Security-key request was cancelled.");
    return serializeAssertion(credential);
  };

  const signIn = async () => {
    busy = "login";
    try {
      const start = await adminApi.authStart(baseUrl, username.trim());
      const finish = await adminApi.authFinish(baseUrl, {
        username: username.trim(),
        flow_id: start.flow_id,
        credential: await getAssertion(start.public_key_credential_request_options),
        totp_code: totpCode.trim(),
      });
      sessionToken = finish.token;
      sessionExpiresAt = finish.expires_at_unix_ms;
      totpCode = "";
      await refresh();
      notice = "Operator session established. It expires after 15 minutes.";
    } catch (error) {
      sessionToken = "";
      notice = `Sign-in failed: ${errorMessage(error)}`;
    } finally {
      busy = "";
    }
  };

  const refresh = async () => {
    if (!sessionToken) return;
    const [nextDashboard, nextSettings, nextAudit] = await Promise.all([
      adminApi.dashboard(baseUrl, sessionToken),
      adminApi.settings(baseUrl, sessionToken),
      adminApi.audit(baseUrl, sessionToken),
    ]);
    dashboard = nextDashboard;
    settings = nextSettings.settings;
    audit = nextAudit.entries;
    drafts = Object.fromEntries(
      settings.map((setting) => [setting.key, String(setting.value)]),
    );
  };

  const reauthenticate = async (): Promise<string> => {
    if (!mutationTotp.trim()) throw new Error("Current operator TOTP is required.");
    const start = await adminApi.reauthStart(baseUrl, sessionToken);
    const finish = await adminApi.reauthFinish(baseUrl, sessionToken, {
      username: username.trim(),
      flow_id: start.flow_id,
      credential: await getAssertion(start.public_key_credential_request_options),
      totp_code: mutationTotp.trim(),
    });
    mutationTotp = "";
    return finish.token;
  };

  const parsedSetting = (setting: RuntimeSetting): unknown => {
    const raw = drafts[setting.key]?.trim() ?? "";
    if (typeof setting.value === "boolean") {
      if (raw !== "true" && raw !== "false") throw new Error("Use true or false.");
      return raw === "true";
    }
    if (!/^\d+$/.test(raw)) throw new Error("Use a non-negative integer.");
    const value = Number(raw);
    if (!Number.isSafeInteger(value)) throw new Error("Value exceeds browser integer range.");
    return value;
  };

  const updateSetting = async (setting: RuntimeSetting) => {
    busy = `setting-${setting.key}`;
    try {
      const reauthToken = await reauthenticate();
      await adminApi.updateSetting(baseUrl, sessionToken, {
        key: setting.key,
        value: parsedSetting(setting),
        expected_version: setting.version,
        reauth_token: reauthToken,
        reason: mutationReason,
        confirmation: settingConfirmation,
      });
      mutationReason = "";
      settingConfirmation = "";
      await refresh();
      notice = `${setting.key} updated and audited.`;
    } catch (error) {
      notice = `Setting update failed: ${errorMessage(error)}`;
    } finally {
      busy = "";
    }
  };

  const changeSuspension = async () => {
    busy = "suspension";
    try {
      const reauthToken = await reauthenticate();
      await adminApi.suspend(baseUrl, sessionToken, {
        user_id: suspensionUserId.trim(),
        suspended: suspensionEnabled,
        reauth_token: reauthToken,
        reason: mutationReason,
        confirmation: suspensionConfirmation,
      });
      mutationReason = "";
      suspensionConfirmation = "";
      await refresh();
      notice = suspensionEnabled ? "Account suspended and sessions revoked." : "Account unsuspended.";
    } catch (error) {
      notice = `Account action failed: ${errorMessage(error)}`;
    } finally {
      busy = "";
    }
  };

  const addSecurityKey = async () => {
    busy = "add-security-key";
    try {
      const reauthToken = await reauthenticate();
      const start = await adminApi.addSecurityKeyStart(baseUrl, sessionToken);
      const credential = (await navigator.credentials.create({
        publicKey: parseCreationOptions(start.public_key_credential_creation_options),
      })) as PublicKeyCredential | null;
      if (!credential) throw new Error("Security-key enrollment was cancelled.");
      await adminApi.addSecurityKeyFinish(baseUrl, sessionToken, {
        flow_id: start.flow_id,
        credential: serializeAttestation(credential),
        name: newSecurityKeyName.trim(),
        reauth_token: reauthToken,
        reason: mutationReason,
        confirmation: securityKeyConfirmation,
      });
      mutationReason = "";
      securityKeyConfirmation = "";
      await refresh();
      notice = "Backup security key enrolled and audited.";
    } catch (error) {
      notice = `Security-key enrollment failed: ${errorMessage(error)}`;
    } finally {
      busy = "";
    }
  };

  const removeSecurityKey = async (keyId: string) => {
    busy = `remove-security-key-${keyId}`;
    try {
      const reauthToken = await reauthenticate();
      await adminApi.removeSecurityKey(baseUrl, sessionToken, {
        key_id: keyId,
        reauth_token: reauthToken,
        reason: mutationReason,
        confirmation: removeSecurityKeyConfirmation,
      });
      mutationReason = "";
      removeSecurityKeyConfirmation = "";
      await signOut();
      notice = "Security key removed, all operator sessions revoked. Sign in again.";
    } catch (error) {
      notice = `Security-key removal failed: ${errorMessage(error)}`;
    } finally {
      busy = "";
    }
  };

  const signOut = async () => {
    const activeToken = sessionToken;
    sessionToken = "";
    sessionExpiresAt = 0;
    dashboard = null;
    settings = [];
    audit = [];
    notice = "Local operator session cleared.";
    if (activeToken) {
      try {
        await adminApi.logout(baseUrl, activeToken);
      } catch {
        // The short-lived server session will expire even if logout cannot reach it.
      }
    }
  };

  const bytes = (value: number) =>
    new Intl.NumberFormat(undefined, { notation: "compact", style: "unit", unit: "byte" }).format(value);
</script>

<svelte:head>
  <title>Kamori Operator Console</title>
  <meta name="robots" content="noindex,nofollow,noarchive" />
</svelte:head>

<main>
  <header>
    <div>
      <p class="eyebrow">Restricted operations</p>
      <h1>Kamori Operator Console</h1>
      <p class="subtitle">Aggregate service controls only. No content, keys, or impersonation.</p>
    </div>
    {#if sessionToken}
      <button class="secondary" on:click={signOut}>Clear session</button>
    {/if}
  </header>

  <p class="notice">{notice}</p>

  {#if !sessionToken}
    <section>
      <h2>Operator authentication</h2>
      <label>Operator username<input bind:value={username} autocomplete="username" /></label>
      <label>Current TOTP<input bind:value={totpCode} inputmode="numeric" autocomplete="one-time-code" /></label>
      <div class="actions">
        <button on:click={signIn} disabled={Boolean(busy)}>Use security key and sign in</button>
      </div>
      <details>
        <summary>First-time enrollment</summary>
        <p>Run <code>cloud-server admin-bootstrap &lt;username&gt;</code> on the trusted ops host. The token expires in 15 minutes.</p>
        <label>One-time bootstrap token<input bind:value={bootstrapToken} type="password" autocomplete="off" /></label>
        <button on:click={enroll} disabled={Boolean(busy)}>Enroll roaming security key</button>
      </details>
    </section>
  {:else}
    <div class="session-line">
      Session expires {new Date(sessionExpiresAt).toLocaleString()}
      <button class="secondary" on:click={refresh} disabled={Boolean(busy)}>Refresh</button>
    </div>

    {#if dashboard}
      <section>
        <h2>Service overview</h2>
        <div class="metrics">
          <article><strong>{dashboard.active_accounts}</strong><span>active accounts</span></article>
          <article><strong>{dashboard.suspended_accounts}</strong><span>suspended</span></article>
          <article><strong>{bytes(dashboard.total_blob_storage_bytes)}</strong><span>ciphertext storage</span></article>
          <article><strong>{dashboard.pending_blobs}</strong><span>pending blobs</span></article>
          <article><strong>{dashboard.registration_enabled ? "open" : "closed"}</strong><span>registration</span></article>
        </div>
        <p class="muted">Migration {dashboard.latest_migration ?? "unknown"} · beta cap {dashboard.beta_account_limit}</p>
        {#if dashboard.jobs.length > 0}
          <div class="rows">
            {#each dashboard.jobs as job}
              <div><strong>{job.job_name}</strong><span class:bad={job.status !== "ok"}>{job.status}</span></div>
            {/each}
          </div>
        {/if}
      </section>

      <section class:danger={dashboard.security_keys.length < 2}>
        <h2>Operator security keys</h2>
        {#if dashboard.security_keys.length < 2}
          <p class="notice">Enrollment is not resilient yet. Add a second roaming key and store it separately before opening registration.</p>
        {/if}
        <div class="rows">
          {#each dashboard.security_keys as key}
            <div>
              <span><strong>{key.name}</strong><small>Added {new Date(key.created_at_unix_ms).toLocaleDateString()}</small></span>
              <code>{key.id}</code>
              <button class="danger-button" on:click={() => removeSecurityKey(key.id)} disabled={Boolean(busy)}>Remove</button>
            </div>
          {/each}
        </div>
        <label>New key name<input bind:value={newSecurityKeyName} /></label>
        <label>Typed confirmation<input bind:value={securityKeyConfirmation} placeholder="ADD SECURITY KEY" /></label>
        <button on:click={addSecurityKey} disabled={Boolean(busy)}>Enroll another roaming key</button>
        <label>Typed removal confirmation<input bind:value={removeSecurityKeyConfirmation} placeholder="REMOVE SECURITY KEY &lt;uuid&gt;" /></label>
        <p class="muted">This action also uses the fresh TOTP, reason, and current security key entered below.</p>
      </section>
    {/if}

    <section>
      <h2>Runtime controls</h2>
      <p class="muted">Every write needs a fresh security-key assertion, TOTP, reason, exact typed confirmation, and optimistic version match.</p>
      <div class="danger-grid">
        <label>Fresh TOTP for next action<input bind:value={mutationTotp} inputmode="numeric" autocomplete="one-time-code" /></label>
        <label>Reason (10–500 characters)<textarea bind:value={mutationReason}></textarea></label>
        <label>Typed setting confirmation<input bind:value={settingConfirmation} placeholder="SET registration_enabled" /></label>
      </div>
      <div class="rows settings">
        {#each settings as setting}
          <div>
            <span><strong>{setting.key}</strong><small>v{setting.version} · {setting.overridden ? "database override" : "deployment default"}</small></span>
            <input bind:value={drafts[setting.key]} aria-label={setting.key} />
            <button on:click={() => updateSetting(setting)} disabled={Boolean(busy)}>Apply</button>
          </div>
        {/each}
      </div>
    </section>

    <section class="danger">
      <h2>Account suspension</h2>
      <p class="muted">Suspension revokes refresh sessions and blocks every authenticated request. The console cannot view user content.</p>
      <label>User UUID<input bind:value={suspensionUserId} /></label>
      <label class="checkbox"><input type="checkbox" bind:checked={suspensionEnabled} /> Suspend (clear to unsuspend)</label>
      <label>Typed confirmation<input bind:value={suspensionConfirmation} placeholder={suspensionEnabled ? `SUSPEND ${suspensionUserId}` : `UNSUSPEND ${suspensionUserId}`} /></label>
      <button class="danger-button" on:click={changeSuspension} disabled={Boolean(busy)}>Apply account state</button>
    </section>

    <section>
      <h2>Audit log</h2>
      <div class="audit">
        {#each audit as entry}
          <article>
            <strong>{entry.event_kind}</strong>
            <span>{new Date(entry.created_at_unix_ms).toLocaleString()} · {entry.actor_username ?? "bootstrap"}</span>
            {#if entry.target_id}<code>{entry.target_kind}: {entry.target_id}</code>{/if}
            {#if entry.reason}<p>{entry.reason}</p>{/if}
          </article>
        {/each}
      </div>
    </section>
  {/if}
</main>

<footer>
  Kamori Operator Console is AGPL-3.0-only and comes without warranty.
  <a href="https://github.com/Kamori-app/kamori-app" target="_blank" rel="noreferrer">Corresponding source and license</a>
</footer>

<style>
  :global(*) { box-sizing: border-box; }
  :global(body) { margin: 0; background: #08111f; color: #e6edf7; font-family: Inter, ui-sans-serif, system-ui, sans-serif; }
  main { width: min(1120px, calc(100% - 32px)); margin: 0 auto; padding: 42px 0 80px; }
  footer { width: min(1120px, calc(100% - 32px)); margin: -55px auto 30px; color: #94a3b8; font-size: .75rem; }
  footer a { color: #bae6fd; }
  header { display: flex; justify-content: space-between; align-items: flex-start; gap: 24px; margin-bottom: 24px; }
  h1 { margin: 4px 0 8px; font-size: clamp(2rem, 5vw, 3.7rem); letter-spacing: -0.045em; }
  h2 { margin: 0 0 16px; font-size: 1.1rem; }
  .eyebrow { margin: 0; color: #7dd3fc; font-size: .75rem; font-weight: 800; letter-spacing: .18em; text-transform: uppercase; }
  .subtitle, .muted { color: #94a3b8; }
  .notice { border: 1px solid #1e3a5f; background: #0c1b2e; border-radius: 12px; padding: 12px 14px; color: #bae6fd; }
  section { margin-top: 18px; padding: 22px; border: 1px solid #243247; border-radius: 18px; background: #0f1a2a; box-shadow: 0 20px 70px #0004; }
  label { display: grid; gap: 6px; margin: 12px 0; color: #aab8cc; font-size: .82rem; }
  input, textarea { width: 100%; border: 1px solid #33445d; border-radius: 10px; background: #08111f; color: #f8fafc; padding: 11px 12px; font: inherit; }
  textarea { min-height: 74px; resize: vertical; }
  button { border: 0; border-radius: 10px; background: #38bdf8; color: #082f49; padding: 10px 14px; font-weight: 800; cursor: pointer; }
  button:disabled { opacity: .45; cursor: wait; }
  button.secondary { background: #1e293b; color: #dbeafe; border: 1px solid #334155; }
  .danger-button { background: #fb7185; color: #4c0519; }
  .actions, .session-line { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .session-line { margin-top: 18px; color: #94a3b8; font-size: .82rem; }
  details { margin-top: 18px; border-top: 1px solid #243247; padding-top: 16px; }
  summary { cursor: pointer; font-weight: 700; }
  code { color: #bae6fd; overflow-wrap: anywhere; }
  .metrics { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 10px; }
  .metrics article { display: grid; gap: 5px; padding: 15px; border-radius: 12px; background: #08111f; }
  .metrics strong { font-size: 1.4rem; }
  .metrics span, small { color: #94a3b8; font-size: .75rem; }
  .rows { display: grid; gap: 8px; margin-top: 14px; }
  .rows > div { display: flex; align-items: center; justify-content: space-between; gap: 12px; border-radius: 10px; background: #08111f; padding: 11px; }
  .rows span { display: grid; gap: 3px; }
  .settings input { max-width: 280px; }
  .bad { color: #fb7185; }
  .danger { border-color: #7f1d3d; }
  .danger-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; }
  .checkbox { display: flex; align-items: center; gap: 8px; }
  .checkbox input { width: auto; }
  .audit { display: grid; gap: 8px; max-height: 520px; overflow: auto; }
  .audit article { display: grid; gap: 4px; padding: 12px; border-radius: 10px; background: #08111f; }
  .audit span { color: #94a3b8; font-size: .75rem; }
  .audit p { margin: 4px 0 0; }
  @media (max-width: 700px) {
    main { width: min(100% - 20px, 1120px); padding-top: 24px; }
    header, .rows > div { align-items: stretch; flex-direction: column; }
    .settings input { max-width: none; }
  }
</style>
