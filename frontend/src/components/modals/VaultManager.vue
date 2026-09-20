<template>
  <v-dialog :model-value="modelValue" max-width="560" @update:model-value="emit('update:modelValue', $event)">
    <v-card>
      <v-card-title class="d-flex align-center">
        Vault Manager
        <v-spacer />
        <v-btn icon="mdi-close" size="small" variant="plain" data-testid="vault-manager-close-btn" @click="close" />
      </v-card-title>

      <v-card-text style="max-height: 400px; overflow-y: auto;">
        <v-list density="compact">
          <v-list-item
            v-for="vault in vaultsStore.vaults"
            :key="vault.id"
            :title="vault.name"
            :subtitle="vault.path"
            :active="vault.id === vaultsStore.activeVaultId"
            active-color="primary"
            @click="vaultsStore.setActiveVault(vault.id)"
          >
            <template #append>
              <v-btn
                icon="mdi-delete-outline"
                size="x-small"
                density="compact"
                variant="plain"
                base-color="error"
                @click.stop="deleteVault(vault.id)"
              />
            </template>
          </v-list-item>
        </v-list>

        <v-divider class="my-3" />

        <!-- Add vault form -->
        <p class="text-caption font-weight-bold mb-2">Add Vault</p>
        <v-text-field v-model="newName" label="Name" density="compact" data-testid="vault-name-input" />
        <div v-if="showPathField" class="d-flex align-center gap-2 mb-1">
          <v-text-field
            v-model="newPath"
            label="Directory path (optional)"
            density="compact"
            hide-details
            class="flex-grow-1"
            data-testid="vault-path-input"
            :readonly="isTauriEnv"
            :placeholder="isTauriEnv ? 'Click Browse… to select a folder' : '/path/to/vault'"
          />
          <v-btn
            v-if="isTauriEnv"
            variant="tonal"
            size="small"
            data-testid="vault-browse-btn"
            @click="browseForVaultDir"
          >Browse…</v-btn>
        </div>
        <div class="text-caption text-medium-emphasis mb-2">
          <template v-if="!showPathField">
            Vault directories are created automatically under the server-configured vault base directory.
            <a href="#" class="text-primary" @click.prevent="showPathField = true">Set a custom path</a>
          </template>
          <template v-else>
            Leave the path blank to use the server default.
            <a href="#" class="text-primary" @click.prevent="showPathField = false; newPath = ''">Use default</a>
          </template>
        </div>

        <v-alert v-if="vaultError" type="error" density="compact" variant="tonal" class="mb-2" closable data-testid="vault-error-alert" @click:close="vaultError = ''">
          {{ vaultError }}
        </v-alert>

        <v-divider class="my-3" />

        <p class="text-caption font-weight-bold mb-2">Sharing &amp; Groups</p>
        <v-alert v-if="sharingError" type="error" density="compact" variant="tonal" class="mb-2">
          {{ sharingError }}
        </v-alert>

        <template v-if="vaultsStore.activeVaultId && canUseGroupsAndSharing">
          <v-list density="compact" class="mb-2" v-if="shares">
            <v-list-subheader>Current access</v-list-subheader>
            <v-list-item>
              <v-list-item-title class="text-caption">
                Owner: {{ shares.owner_user_id ?? 'Unknown' }}
              </v-list-item-title>
            </v-list-item>
            <v-list-item
              v-for="share in shares.user_shares"
              :key="`user-${share.principal_id}-${share.role}`"
              :title="`User: ${share.principal_name}`"
              :subtitle="`Role: ${share.role}`"
            >
              <template #append>
                <v-btn
                  icon="mdi-account-minus-outline"
                  size="x-small"
                  variant="plain"
                  title="Revoke user share"
                  :loading="sharingBusy"
                  @click="revokeUserShare(share.principal_id)"
                />
              </template>
            </v-list-item>
            <v-list-item
              v-for="share in shares.group_shares"
              :key="`group-${share.principal_id}-${share.role}`"
              :title="`Group: ${share.principal_name}`"
              :subtitle="`Role: ${share.role}`"
            >
              <template #append>
                <v-btn
                  icon="mdi-account-group-outline"
                  size="x-small"
                  variant="plain"
                  title="Revoke group share"
                  :loading="sharingBusy"
                  @click="revokeGroupShare(share.principal_id)"
                />
              </template>
            </v-list-item>
          </v-list>

          <v-row dense class="mb-2">
            <v-col cols="12" md="6">
              <v-text-field
                v-model="shareUsername"
                label="Share with user"
                density="compact"
                placeholder="username"
                hide-details
              />
            </v-col>
            <v-col cols="8" md="4">
              <v-select
                v-model="shareUserRole"
                :items="roleOptions"
                item-title="title"
                item-value="value"
                label="Role"
                density="compact"
                hide-details
              />
            </v-col>
            <v-col cols="4" md="2">
              <v-btn block color="primary" variant="tonal" :loading="sharingBusy" @click="shareWithUser">
                Share
              </v-btn>
            </v-col>
          </v-row>

          <v-row dense class="mb-2">
            <v-col cols="12" md="6">
              <v-select
                v-model="shareGroupId"
                :items="groups"
                item-title="name"
                item-value="id"
                label="Share with group"
                density="compact"
                hide-details
              />
            </v-col>
            <v-col cols="8" md="4">
              <v-select
                v-model="shareGroupRole"
                :items="roleOptions"
                item-title="title"
                item-value="value"
                label="Role"
                density="compact"
                hide-details
              />
            </v-col>
            <v-col cols="4" md="2">
              <v-btn block color="primary" variant="tonal" :loading="sharingBusy" @click="shareWithGroup">
                Share
              </v-btn>
            </v-col>
          </v-row>

          <v-row dense class="mb-2">
            <v-col cols="9">
              <v-text-field
                v-model="newGroupName"
                label="Create group"
                density="compact"
                placeholder="group-name"
                hide-details
              />
            </v-col>
            <v-col cols="3">
              <v-btn block color="primary" variant="tonal" :loading="sharingBusy" @click="createGroup">
                Create
              </v-btn>
            </v-col>
          </v-row>

          <v-select
            v-model="memberGroupId"
            :items="groups"
            item-title="name"
            item-value="id"
            label="Manage group members"
            density="compact"
            class="mb-2"
            hide-details
          />

          <template v-if="memberGroupId">
            <v-list density="compact" class="mb-2" v-if="groupMembers.length > 0">
              <v-list-item
                v-for="member in groupMembers"
                :key="member.user_id"
                :title="member.username"
                :subtitle="member.user_id"
              >
                <template #append>
                  <v-btn
                    icon="mdi-account-remove-outline"
                    size="x-small"
                    variant="plain"
                    @click="removeMember(member.user_id)"
                  />
                </template>
              </v-list-item>
            </v-list>
            <div v-else class="text-caption text-medium-emphasis mb-2">No members in this group yet.</div>

            <v-row dense>
              <v-col cols="9">
                <v-text-field
                  v-model="newMemberUsername"
                  label="Add member by username"
                  density="compact"
                  hide-details
                />
              </v-col>
              <v-col cols="3">
                <v-btn
                  block
                  color="primary"
                  variant="tonal"
                  :loading="sharingBusy"
                  data-testid="group-member-add-btn"
                  @click="addMember"
                >
                  Add
                </v-btn>
              </v-col>
            </v-row>
          </template>
        </template>
        <div v-else-if="vaultsStore.activeVaultId" class="text-caption text-medium-emphasis" data-testid="sharing-unavailable-offline">
          Sharing and groups aren't available offline.
        </div>
        <div v-else class="text-caption text-medium-emphasis">
          Select a vault first to manage sharing.
        </div>
      </v-card-text>

      <v-card-actions>
        <v-spacer />
        <v-btn @click="close">Close</v-btn>
        <v-btn color="primary" :disabled="!newName" :loading="saving" data-testid="add-vault-btn" @click="addVault">Add</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue';
import { useVaultsStore } from '@/stores/vaults';
import {
  apiAddGroupMember,
  apiCreateGroup,
  apiListGroupMembers,
  apiListGroups,
  apiListVaultShares,
  apiRevokeVaultGroupShare,
  apiRevokeVaultUserShare,
  apiRemoveGroupMember,
  apiShareVaultWithGroup,
  apiShareVaultWithUser,
} from '@/api/client';
import type { GroupInfo, GroupMember, VaultRole, VaultShareList } from '@/api/types';
import { isTauri, openDirectoryDialog } from '@/utils/tauri';
import { useCapabilities } from '@/composables/useCapabilities';

const props = defineProps<{ modelValue: boolean }>();
const emit = defineEmits<{ 'update:modelValue': [v: boolean] }>();

const vaultsStore = useVaultsStore();
const newName = ref('');
const newPath = ref('');
const showPathField = ref(false);
const isTauriEnv = isTauri();
const { canUseGroupsAndSharing } = useCapabilities();
const saving = ref(false);
const vaultError = ref('');
const sharingBusy = ref(false);
const sharingError = ref('');

const groups = ref<GroupInfo[]>([]);
const groupMembers = ref<GroupMember[]>([]);
const shares = ref<VaultShareList | null>(null);

const shareUsername = ref('');
const shareUserRole = ref<VaultRole>('viewer');
const shareGroupId = ref<string | null>(null);
const shareGroupRole = ref<VaultRole>('viewer');

const newGroupName = ref('');
const memberGroupId = ref<string | null>(null);
const newMemberUsername = ref('');

const roleOptions: Array<{ title: string; value: VaultRole }> = [
  { title: 'Viewer', value: 'viewer' },
  { title: 'Editor', value: 'editor' },
  { title: 'Owner', value: 'owner' },
];

function close() {
  emit('update:modelValue', false);
}

watch(
  () => [props.modelValue, vaultsStore.activeVaultId] as const,
  async ([open]) => {
    if (!open) return;
    await loadSharingContext();
  },
  { immediate: true },
);

watch(memberGroupId, async (groupId) => {
  if (!groupId) {
    groupMembers.value = [];
    return;
  }
  try {
    groupMembers.value = await apiListGroupMembers(groupId);
  } catch (e: any) {
    sharingError.value = e?.message ?? 'Failed to load group members.';
  }
});

async function addVault() {
  if (!newName.value) return;
  saving.value = true;
  vaultError.value = '';
  try {
    await vaultsStore.createVault({
      name: newName.value,
      path: newPath.value.trim() || undefined,
    });
    newName.value = '';
    newPath.value = '';
    showPathField.value = false;
  } catch (e: any) {
    vaultError.value = e?.message ?? 'Failed to create vault.';
  } finally {
    saving.value = false;
  }
}

async function browseForVaultDir() {
  const selected = await openDirectoryDialog();
  if (selected) {
    newPath.value = selected;
    showPathField.value = true;
  }
}

async function deleteVault(id: string) {
  if (!confirm('Delete this vault? (Files are not deleted from disk)')) return;
  await vaultsStore.deleteVault(id);
}

async function loadSharingContext() {
  sharingError.value = '';
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId) {
    groups.value = [];
    shares.value = null;
    groupMembers.value = [];
    return;
  }

  try {
    const [nextGroups, nextShares] = await Promise.all([
      apiListGroups(),
      apiListVaultShares(vaultId),
    ]);
    groups.value = nextGroups;
    shares.value = nextShares;

    if (memberGroupId.value && nextGroups.every((g) => g.id !== memberGroupId.value)) {
      memberGroupId.value = null;
    }
    if (shareGroupId.value && nextGroups.every((g) => g.id !== shareGroupId.value)) {
      shareGroupId.value = null;
    }
  } catch (e: any) {
    sharingError.value = e?.message ?? 'Failed to load sharing information.';
  }
}

async function shareWithUser() {
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId || !shareUsername.value.trim()) return;

  sharingBusy.value = true;
  sharingError.value = '';
  try {
    shares.value = await apiShareVaultWithUser(vaultId, {
      username: shareUsername.value.trim(),
      role: shareUserRole.value,
    });
    shareUsername.value = '';
  } catch (e: any) {
    sharingError.value = e?.message ?? 'Failed to share vault with user.';
  } finally {
    sharingBusy.value = false;
  }
}

async function shareWithGroup() {
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId || !shareGroupId.value) return;

  sharingBusy.value = true;
  sharingError.value = '';
  try {
    shares.value = await apiShareVaultWithGroup(vaultId, {
      group_id: shareGroupId.value,
      role: shareGroupRole.value,
    });
  } catch (e: any) {
    sharingError.value = e?.message ?? 'Failed to share vault with group.';
  } finally {
    sharingBusy.value = false;
  }
}

async function createGroup() {
  if (!newGroupName.value.trim()) return;

  sharingBusy.value = true;
  sharingError.value = '';
  try {
    const group = await apiCreateGroup({ name: newGroupName.value.trim() });
    groups.value = [...groups.value, group];
    if (!memberGroupId.value) memberGroupId.value = group.id;
    newGroupName.value = '';
  } catch (e: any) {
    sharingError.value = e?.message ?? 'Failed to create group.';
  } finally {
    sharingBusy.value = false;
  }
}

async function addMember() {
  if (!memberGroupId.value || !newMemberUsername.value.trim()) return;

  sharingBusy.value = true;
  sharingError.value = '';
  try {
    groupMembers.value = await apiAddGroupMember(memberGroupId.value, {
      username: newMemberUsername.value.trim(),
    });
    newMemberUsername.value = '';
  } catch (e: any) {
    sharingError.value = e?.message ?? 'Failed to add group member.';
  } finally {
    sharingBusy.value = false;
  }
}

async function removeMember(userId: string) {
  if (!memberGroupId.value) return;

  sharingBusy.value = true;
  sharingError.value = '';
  try {
    await apiRemoveGroupMember(memberGroupId.value, userId);
    groupMembers.value = groupMembers.value.filter((member) => member.user_id !== userId);
  } catch (e: any) {
    sharingError.value = e?.message ?? 'Failed to remove group member.';
  } finally {
    sharingBusy.value = false;
  }
}

async function revokeUserShare(userId: string) {
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId) return;

  sharingBusy.value = true;
  sharingError.value = '';
  try {
    shares.value = await apiRevokeVaultUserShare(vaultId, userId);
  } catch (e: any) {
    sharingError.value = e?.message ?? 'Failed to revoke user share.';
  } finally {
    sharingBusy.value = false;
  }
}

async function revokeGroupShare(groupId: string) {
  const vaultId = vaultsStore.activeVaultId;
  if (!vaultId) return;

  sharingBusy.value = true;
  sharingError.value = '';
  try {
    shares.value = await apiRevokeVaultGroupShare(vaultId, groupId);
  } catch (e: any) {
    sharingError.value = e?.message ?? 'Failed to revoke group share.';
  } finally {
    sharingBusy.value = false;
  }
}
</script>
