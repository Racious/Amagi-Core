import { defineStore } from 'pinia'
import { ref } from 'vue'
import { api, type ProjectInfo, type InitResult, type ScanResult } from '../api/tauriCommands'

export const useProjectStore = defineStore('project', () => {
  const projects = ref<ProjectInfo[]>([])
  const selectedProjectId = ref<string | null>(null)
  const lastScan = ref<ScanResult | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)

  const selectedProject = () => projects.value.find(p => p.id === selectedProjectId.value) ?? null

  async function fetchProjects() {
    loading.value = true
    error.value = null
    try {
      projects.value = await api.listProjects()
    } catch (e) {
      error.value = String(e)
    } finally {
      loading.value = false
    }
  }

  async function addProject(path: string): Promise<ProjectInfo> {
    loading.value = true
    error.value = null
    try {
      const info = await api.addProject(path)
      projects.value.push(info)
      return info
    } catch (e) {
      error.value = String(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  async function initProject(projectId: string): Promise<InitResult> {
    loading.value = true
    error.value = null
    try {
      const result = await api.initProject(projectId)
      await fetchProjects()
      return result
    } catch (e) {
      error.value = String(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  async function removeProject(projectId: string) {
    loading.value = true
    error.value = null
    try {
      await api.removeProject(projectId)
      projects.value = projects.value.filter(p => p.id !== projectId)
      if (selectedProjectId.value === projectId) selectedProjectId.value = null
    } catch (e) {
      error.value = String(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  async function scanProject(projectId: string): Promise<ScanResult> {
    loading.value = true
    error.value = null
    try {
      lastScan.value = await api.scanProject(projectId)
      return lastScan.value
    } catch (e) {
      error.value = String(e)
      throw e
    } finally {
      loading.value = false
    }
  }

  return { projects, selectedProjectId, lastScan, loading, error, selectedProject, fetchProjects, addProject, initProject, removeProject, scanProject }
})
