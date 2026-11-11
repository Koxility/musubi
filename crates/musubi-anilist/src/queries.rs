//! GraphQL-Dokumente. Rund ein Dutzend Operationen rechtfertigen keinen Codegen-Schritt.

pub const MEDIA_FIELDS: &str = "
fragment MediaFields on Media {
  id idMal type synonyms format status episodes chapters volumes seasonYear
  title { romaji english native }
  startDate { year }
  coverImage { large }
  description(asHtml: false)
  genres averageScore
  nextAiringEpisode { airingAt episode }
}";

pub const ENTRY_FIELDS: &str = "
fragment EntryFields on MediaList {
  id status score(format: POINT_100) progress progressVolumes repeat priority private
  hiddenFromStatusLists notes customLists advancedScores updatedAt
  startedAt { year month day }
  completedAt { year month day }
  media { ...MediaFields }
}";

pub const VIEWER: &str = "
query {
  Viewer {
    id name
    mediaListOptions {
      scoreFormat
      animeList { customLists advancedScoring }
      mangaList { customLists advancedScoring }
    }
  }
}";

/// Profile and statistics. The GUI sorts; AniList returns every group. The score format
/// is needed to read `scores`, which AniList reports in the user's own scale.
pub const PROFILE: &str = "
query {
  Viewer {
    name siteUrl bannerImage
    avatar { large }
    mediaListOptions { scoreFormat }
    statistics {
      anime {
        count meanScore standardDeviation minutesWatched episodesWatched
        statuses { status count }
        scores { score count }
        formats { format count meanScore minutesWatched }
        genres { genre count meanScore minutesWatched }
        releaseYears { releaseYear count meanScore minutesWatched }
      }
      manga {
        count meanScore standardDeviation chaptersRead volumesRead
        statuses { status count }
        scores { score count }
        formats { format count meanScore chaptersRead }
        genres { genre count meanScore chaptersRead }
        releaseYears { releaseYear count meanScore chaptersRead }
      }
    }
  }
}";

pub const LIST_COLLECTION: &str = "
query ($userId: Int, $type: MediaType, $chunk: Int) {
  MediaListCollection(userId: $userId, type: $type, chunk: $chunk, perChunk: 500) {
    hasNextChunk
    lists { entries { ...EntryFields } }
  }
}";

pub const SEARCH: &str = "
query ($search: String, $type: MediaType, $formats: [MediaFormat], $perPage: Int) {
  Page(page: 1, perPage: $perPage) {
    media(search: $search, type: $type, format_in: $formats, sort: SEARCH_MATCH) {
      ...MediaFields
    }
  }
}";

pub const MEDIA_BY_ID: &str = "
query ($id: Int) { Media(id: $id) { ...MediaFields } }";

pub const MEDIA_BY_MAL: &str = "
query ($idMal: Int, $type: MediaType) { Media(idMal: $idMal, type: $type) { ...MediaFields } }";

pub const RELATIONS: &str = "
query ($id: Int) {
  Media(id: $id) {
    ...MediaFields
    relations { edges { relationType(version: 2) node { ...MediaFields } } }
  }
}";

pub const SAVE_ENTRY: &str = "
mutation (
  $mediaId: Int, $status: MediaListStatus, $scoreRaw: Int, $progress: Int,
  $progressVolumes: Int, $repeat: Int, $priority: Int, $private: Boolean,
  $hiddenFromStatusLists: Boolean, $notes: String, $customLists: [String],
  $advancedScores: [Float], $startedAt: FuzzyDateInput, $completedAt: FuzzyDateInput
) {
  SaveMediaListEntry(
    mediaId: $mediaId, status: $status, scoreRaw: $scoreRaw, progress: $progress,
    progressVolumes: $progressVolumes, repeat: $repeat, priority: $priority, private: $private,
    hiddenFromStatusLists: $hiddenFromStatusLists, notes: $notes, customLists: $customLists,
    advancedScores: $advancedScores, startedAt: $startedAt, completedAt: $completedAt
  ) { ...EntryFields }
}";

pub const DELETE_ENTRY: &str = "
mutation ($id: Int) { DeleteMediaListEntry(id: $id) { deleted } }";

pub const UPDATE_ANIME_LISTS: &str = "
mutation ($lists: [String]) {
  UpdateUser(animeListOptions: { customLists: $lists }) { id }
}";

pub const UPDATE_MANGA_LISTS: &str = "
mutation ($lists: [String]) {
  UpdateUser(mangaListOptions: { customLists: $lists }) { id }
}";
