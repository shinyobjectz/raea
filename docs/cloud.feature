@spec
Feature: raea cloud: raea and an ACP agent on a server of your own (sdk/cloud)
  Shane, 2026-11-10: "getting the kpop concept and its docker image up and running as a true module called cloud and
  would support the many configurations we could set up for deployment of raea engine to the cloud with acp agent.
  this is more of a developer thing for now".
  kpop proved the idea on one Fly Sprite: an agent in the cloud next to raea, spoken to over ACP through a small relay,
  with a TypeScript client. The cloud node image (apps/site/api/container) proved raea renders in a Linux container
  on Mesa's llvmpipe. sdk/cloud makes them one module a developer deploys on their own account, wherever they run
  containers. raea sells no cloud compute: the server, the GPU and the agent's subscription are the developer's.
  Shane, later that day: "i mean the point is we provide a way to deploy raea to gpu and cpu". So one image runs on
  either and says which it's using, a GPU is a deploy target proven on a real NVIDIA card, and a cheap always-on CPU
  box (the agent's) hands its renders to a GPU worker that sleeps when idle.

  Background:
    Given one image, raea-cloud: Node 24 on Debian trixie, raea's Linux release (x86_64, checked against its .sha256),
      the server (sdk/cloud/server) and the agents named when it was built (AGENTS, ACP registry ids)
    And one secret, RAEA_CLOUD_TOKEN, that every request but /healthz carries

  Rule: the image holds raea and any ACP agent
    Scenario: CL1. The image is built from a raea release
      Then `docker build --build-arg RAEA_VERSION=V sdk/cloud` downloads raea V's Linux tarball, checks it against
        the release's .sha256 (and RAEA_SHA256 when given), and `raea --version` in the image says V
      And it runs as a user named raea, the workspace ~/work in raea's home folder
    Scenario: CL2. Agents come from the ACP registry by id
      Given AGENTS="kimi-code claude-acp codex-acp gemini opencode" (any ids the registry lists)
      Then each is installed from its registry entry when the image is built: an npx entry with npm at the registry's
        version, a binary entry from its linux archive checked by sha256; a uvx entry is refused with a reason
      And /opt/raea-cloud/agents/ID/spawn.json says how to start it, and the registry's entry is kept beside it
      And RAEA_AGENT picks the default agent; RAEA_AGENT_CMD runs any other ACP agent by its command line
    Scenario: CL3. Each agent gets raea's skill, matched to the image's raea
      Then on every start `raea skills install --agent A` runs for each installed agent raea has skills for (kimi,
        claude, codex, gemini), so a mounted home from an older image is brought up to date

  Rule: one port, one token
    Scenario: CL4. The server answers on :8080
      Then GET /healthz is "ok" without a token, for platform health checks
      And GET /v1/health says raea's version, the default agent, the installed agents, the workspace, the device raea
        renders on (CL13), the GPU worker it hands renders to (CL15) and the render jobs it holds (CL14)
      And every other route refuses a request without the token (Authorization: Bearer, or ?token= for a browser's
        WebSocket) with 401, compared in constant time, and the server won't start without one
    Scenario: CL5. Files go in and out of the workspace
      Then PUT /v1/files?path=P writes P (its folders made), GET reads it or lists a folder as JSON
      And a path that leaves the workspace (.., an absolute path, a symlink out) is refused with 403
    Scenario: CL6. raea runs without an agent
      Then POST /v1/run {args, cwd} runs `raea ARGS` in the workspace, streaming {out}, {err} and last {exit} lines
      And the token and the state keys are never in its environment, and a dropped request stops the run
    Scenario: CL6b. A shell for an agent the server doesn't run (raea cloud, 11-17)
      Then POST /v1/shell {command, cwd, timeout_ms} runs a bash command line in the workspace, streaming the same
        {out}, {err} and last {exit} lines
      And past its timeout (10 minutes unless given, an hour at most), or when the request drops, its whole process
        group is stopped, and a timeout's last line says timed_out
      And an empty command is a 400 and a cwd outside the workspace a 403; test/smoke.ts checks all three
    Scenario: CL7. ACP over a WebSocket
      Then each WebSocket on /v1/acp?agent=ID is one agent process in the workspace: a text frame is one JSON-RPC
        message to its stdin, each line it prints is a frame back, and closing either end ends the other
      And the agent's environment is the container's (its API keys) without RAEA_CLOUD_TOKEN or the state keys
    Scenario: CL8. Agents that sign in by a device code sign in through the server
      Then POST /v1/login?agent=kimi-code starts Kimi Code's login and returns the URL to approve; the login is kept
        in the home folder
      And agents that take a key (claude-acp, codex-acp, gemini) take it from the environment

  Rule: it deploys anywhere containers run
    Scenario: CL9. Docker and Compose
      Then deploy/compose.yaml runs it with the home folder on a named volume, and deploy/compose.gpu.yaml adds an
        NVIDIA GPU (graphics capability, so Vulkan sees it)
    Scenario: CL10. The platforms
      Then deploy/ has a Fly Machines app (a volume, stopped when idle, woken by a request) and Kubernetes manifests
        (a PersistentVolumeClaim, a Secret, a Deployment and a Service)
      And where the disk doesn't outlive the container (Cloudflare Containers, Cloud Run), RAEA_CLOUD_STATE names an
        S3-compatible object (R2, S3) the home folder is restored from on start and saved to on stop
    Scenario: CL10b. Cloudflare Containers
      Then deploy/ has a Cloudflare Container config: a Worker in front, the container asleep when idle, its home in R2
    Scenario: CL11. The TypeScript client
      Then `new RaeaCloud({ url, token })` from sdk/cloud/src has health, read, write, list, run, login and session;
        a session is ACP: prompt(text, onUpdate) resolves with the stop reason, permission requests go to
        onPermission (allowed once by default: the container is the sandbox)
      And it runs on Node 22+, Bun and in a browser (fetch and WebSocket only)

  Rule: it renders on a GPU or a CPU
    Scenario: CL13. One image for both, and it says which
      Given a container given an NVIDIA GPU (Compose's gpu overlay, Kubernetes' nvidia.com/gpu, Modal), an AMD card in
        /dev/dri, or none
      Then the entrypoint opens NVIDIA's Vulkan driver when the host passed one in (the host's ICD file, else the
        image's, naming the library the NVIDIA runtime mounts), and raea renders on it; otherwise on Mesa (radv for
        an AMD card, llvmpipe on the CPU)
      And RAEA_CLOUD_DEVICE=gpu makes a GPU that doesn't open an error at start, never a quiet fall back to the CPU;
        RAEA_CLOUD_DEVICE=cpu keeps raea on llvmpipe
      And /v1/health's device says where renders run (gpu or cpu), on which device, and NVIDIA's own report
      And test/gpu/edit.tsx (576 lit, shadowed blocks, 1920x1080, 90 frames) times the same render on the GPU and on
        the CPU of one server
    Scenario: CL14. A render job, and a GPU that sleeps when idle
      Then POST /v1/jobs?args=JSON takes a project as a tar (zstd, gzip or none) and runs `raea ARGS` on it in a folder
        of its own; GET /v1/jobs/ID?from=N follows its output in requests of 20 s at most (a serverless platform's
        proxy cuts longer ones); GET /v1/jobs/ID/out returns the files it made or changed, and the job is dropped
      And deploy/modal/raea_cloud.py runs the image on a Modal GPU (an L4, else an A10G, else a T4, unless
        RAEA_CLOUD_GPU names others or none), one container that starts on a request and stops after five idle
        minutes, the token in a Modal secret
    Scenario: CL15. A CPU box hands its renders to a GPU worker
      Given RAEA_CLOUD_GPU_URL and RAEA_CLOUD_GPU_TOKEN on a CPU box (the agent's, always on, cheap)
      Then `raea render` there, from the agent, its shell or /v1/run, sends the render's project to the worker as a
        job and lands what it made back in the project; its paths are made relative to the project, an output outside
        it is moved to its place after, and an input outside it renders on the box with a line saying why
      And a worker that can't be reached fails the render unless RAEA_CLOUD_GPU_FALLBACK=cpu
      And every other raea command runs on the box

  Rule: it is proven end to end
    Scenario: CL12. A stand-in agent, then a real one
      Then test/smoke.sh builds the image, starts it, and checks CL4-CL7, CL14 and CL15 with test/echo-agent.mjs (a
        scripted ACP agent): the token refused, a path escape refused, a frame rendered by /v1/run, a render job's file
        back, a second container handing its render to the first, an ACP turn round-tripped
      And with Kimi Code signed in, one real turn renders a title card with raea in the container, and the file
        comes back through /v1/files
