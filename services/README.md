This folder is an example managed-service inventory for app/reference repos that embed Service Lasso.

Important distinction:
- the root `service.json` is still the canonical manifest for the template service repo itself
- the manifests under `services/` are example inventory entries showing what a host/app repo should carry when it wants to manage a small baseline stack through Service Lasso

Current baseline inventory:
- `echo-service`
- `service-admin`
- `@node`
- `@traefik`

If a host/app repo includes `service-admin`, it should also include the manifests needed to satisfy Service Admin's declared service dependencies.
